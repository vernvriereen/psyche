use crate::{
    state::{train::FinishedTrainers, types::DeserializeError},
    ClientTUIState, TrainingResult,
};

use psyche_coordinator::{Committee, Coordinator, RunState, Witness};
use psyche_core::{sha256, NodeIdentity};
use psyche_modeling::{DistroResult, Trainer};
use psyche_network::{AuthenticatableIdentity, BlobTicket, Hash, TransmittableDistroResult};
use std::{collections::HashMap, fmt, sync::Arc};
use tch::TchError;
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{self},
        Notify,
    },
    task::JoinHandle,
};
use tracing::{debug, error, info, info_span, trace, warn, Instrument};
use wandb::DataValue;

use super::{
    cooldown::{CooldownError, CooldownStep, CooldownStepMetadata},
    evals::EvalError,
    init::InitRunError,
    round_state::RoundState,
    stats::StatsLogger,
    train::{TrainError, TrainingStep, TrainingStepMetadata},
    types::PayloadState,
    warmup::{WarmupStep, WarmupStepMetadata},
    witness::{WitnessStep, WitnessStepMetadata, WitnessingError},
    RunInitConfigAndIO,
};

pub struct StepStateMachine<T: NodeIdentity, A: AuthenticatableIdentity> {
    identity: T,

    stats_logger: StatsLogger,

    warmup: WarmupStepMetadata,
    training: TrainingStepMetadata<T, A>,
    witness: WitnessStepMetadata<T>,
    cooldown: CooldownStepMetadata,

    active_step: ActiveStep,

    tx_request_download: mpsc::UnboundedSender<(BlobTicket, u32)>,
    tx_witness: mpsc::UnboundedSender<Witness>,

    notify_try_opportunistic_witness: Arc<Notify>,

    current_round: RoundState<T>,
    previous_round: RoundState<T>,

    coordinator_state: Coordinator<T>,

    node_info: HashMap<String, DataValue>,
}

#[derive(Error, Debug)]
pub enum StepError {
    #[error("Desync: we're in step {active_step} but next RunState is {run_state}")]
    Desync {
        active_step: String,
        run_state: RunState,
    },

    #[error("Witness error: {0}")]
    Witness(#[from] WitnessingError),

    #[error("Cooldown error: {0}")]
    Cooldown(#[from] CooldownError),

    #[error("Train error: {0}")]
    Train(#[from] TrainError),

    #[error("Evals error: {0}")]
    Evals(#[from] EvalError),
}

#[derive(Error, Debug)]
pub enum ApplyMessageError {
    #[error("Failed to put blob up for download")]
    StartDownloadBlob,
}

#[derive(Error, Debug)]
pub enum OpportunisticWitnessError {
    #[error("Failed to send opportunistic witness, channel must be closed")]
    Send,
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> StepStateMachine<T, A> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: T,
        warmup: WarmupStepMetadata,
        training: TrainingStepMetadata<T, A>,
        witness: WitnessStepMetadata<T>,
        cooldown: CooldownStepMetadata,
        trainers: Vec<Trainer>,
        coordinator_state: Coordinator<T>,
        tx_request_download: mpsc::UnboundedSender<(BlobTicket, u32)>,
        tx_witness: mpsc::UnboundedSender<Witness>,
        stats_logger: StatsLogger,
    ) -> Self {
        let mut previous_round = RoundState::default();
        let mut current_round = RoundState::default();

        let active_step =
            ActiveStep::Warmup(warmup.start(trainers, &mut previous_round, &mut current_round));
        let notify_try_opportunistic_witness = Arc::new(Notify::new());
        Self {
            identity,

            stats_logger,

            warmup,
            training,
            witness,
            cooldown,
            active_step,

            current_round,
            previous_round,

            tx_request_download,
            tx_witness,
            notify_try_opportunistic_witness,

            coordinator_state,

            node_info: HashMap::new(),
        }
    }

    pub async fn try_send_opportunistic_witness(
        &mut self,
    ) -> Result<(), OpportunisticWitnessError> {
        if let Some(committee_info) = &self.current_round.committee_info {
            // are we a witness this round?
            if committee_info.1.witness.is_true() {
                if let ActiveStep::Training(step) = &self.active_step {
                    if step.finished() && self.previous_round.batch_ids_not_yet_trained_on.is_none()
                    {
                        // Finished training and finished downloading the previous round's results
                        // (or we're on the first or last which has nothing to download)

                        // check that all batches from the previous round are done deserializing
                        for batch in &self.previous_round.downloads {
                            match batch.1 {
                                PayloadState::Deserializing(thread) if thread.is_finished() => {
                                    // this batch is done deserializing, we can witness on it now.
                                }

                                // we're still downloading or deserializing this batch, so we're not ready to send an opportunistic witness.
                                // this function will get called again when a deserialize finishes.
                                _ => {
                                    return Ok(());
                                }
                            }
                        }

                        if let Some(witness) = WitnessStep::get_witness_to_send(
                            &mut self.previous_round,
                            &mut self.current_round,
                        ) {
                            debug!("Sending opportunistic witness");

                            self.tx_witness
                                .send(witness)
                                .map_err(|_| OpportunisticWitnessError::Send)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn apply_message(
        &mut self,
        from_client_id: T,
        training_result: TrainingResult,
    ) -> Result<(), ApplyMessageError> {
        let result_step = training_result.step;
        let batch_id = training_result.batch_id;
        let round_state = {
            if self.current_round.data_assignments.contains_key(&batch_id) {
                debug!(
                    "Got result gossip for current step {} batch {batch_id}",
                    result_step
                );
                &mut self.current_round
            } else if self.previous_round.data_assignments.contains_key(&batch_id) {
                debug!(
                    "Got result gossip for previous step {} batch {batch_id}",
                    result_step - 1
                );
                &mut self.previous_round
            } else {
                debug!(
                    "Unknown round for gossiped batch id {}, says its for step {} but not in data assignments for our current round (step {}) or previous round (step {})",
                    batch_id, result_step, self.current_round.step, self.previous_round.step,
                );
                return Ok(());
            }
        };

        let check_committee = from_client_id != self.identity;
        if check_committee {
            match &round_state.committee_info {
                Some((_, _, committee_info)) => {
                    if !committee_info.verify_committee_for_client(
                        &from_client_id,
                        &training_result.proof,
                        &self.coordinator_state.epoch_state.clients,
                    ) {
                        debug!("Committee verification failed for commitment 0x{} (step={},batch_id={}) received from {}", hex::encode(training_result.commitment),
                            training_result.step,
                            training_result.batch_id,
                            from_client_id);
                        return Ok(());
                    }
                }
                None => {
                    return Ok(());
                }
            };
        }

        if training_result.proof.committee != Committee::Trainer {
            todo!(
                "broadcast not implemented for committee member {}",
                training_result.proof.committee
            );
        }

        let ticket = training_result.ticket.clone();
        let hash = ticket.hash();

        if round_state.downloads.contains_key(&hash) {
            trace!("Already have downloaded batch id {batch_id}, ignoring duplicated gossip");
            return Ok(());
        }

        let client_commitments = *round_state
            .commitments_per_client
            .get(&from_client_id)
            .unwrap_or(&0);

        let correct_assignee = match round_state.data_assignments.get(&training_result.batch_id) {
            Some(assignee) => from_client_id == *assignee,
            None => false,
        };
        if !correct_assignee {
            warn!(
                    "Got batch {} from {} but they were not assigneed to that data, dropping message 0x{}",
                    training_result.batch_id,
                    from_client_id,
                    hex::encode(training_result.commitment)
                );
            return Ok(());
        }

        round_state
            .commitments_per_client
            .insert(from_client_id, client_commitments + 1);

        let total_commitments = round_state
            .commitments_per_client
            .values()
            .fold(0, |acc, ele| acc + *ele);

        debug!(
            "Total commitments for round {}: {}",
            round_state.height, total_commitments
        );

        round_state
            .results
            .entry(training_result.batch_id)
            .or_default();
        let batch_id = training_result.batch_id;
        round_state
            .results
            .get_mut(&training_result.batch_id)
            .unwrap()
            .push((from_client_id, training_result));
        let download_state = PayloadState::Downloading((from_client_id, batch_id, ticket.clone()));
        round_state.downloads.insert(hash, download_state);

        // start downloading the payload unless this is a self-message
        // (assuming the caller will put our payload in the proper place)
        if from_client_id != self.identity {
            debug!(
                "Requesting download of round {} batch {}: {}",
                round_state.height,
                batch_id,
                ticket.hash()
            );

            self.tx_request_download
                .send((ticket, result_step))
                .map_err(|_| ApplyMessageError::StartDownloadBlob)?;
        }

        Ok(())
    }

    pub async fn apply_distro_result(
        &mut self,
        hash: Hash,
        distro_result: TransmittableDistroResult,
        self_result: Option<Vec<DistroResult>>,
    ) {
        let round_state = if self.current_round.downloads.contains_key(&hash) {
            trace!(
                "Got download {hash} for current round {}",
                self.current_round.height
            );
            &mut self.current_round
        } else if self.previous_round.downloads.contains_key(&hash) {
            trace!(
                "Got download {hash} for previous round {}",
                self.previous_round.height
            );
            &mut self.previous_round
        } else {
            debug!("Unknown download {}", hash);
            return;
        };

        if let Some(self_result) = self_result {
            debug!(
                "Processing our own distro result for batch {} in step {} with hash {hash}",
                distro_result.batch_id, distro_result.step
            );

            round_state.self_distro_results.push(self_result);
        } else {
            debug!(
                "Finished download of distro result for batch {} in step {} with hash {hash}",
                distro_result.batch_id, distro_result.step
            );
        }

        let (from, batch_id, _) = match round_state.downloads.get(&hash) {
            Some(PayloadState::Downloading(x)) => x,
            Some(PayloadState::Deserializing(_)) => {
                debug!("Duplicate download of {}", hash);
                return;
            }
            None => {
                debug!("Unknown download {}", hash);
                return;
            }
        };

        let commitments = match round_state.results.get(batch_id) {
            Some(commitments) => commitments,
            None => {
                info!(
                    "No commitment for payload from {} for batch {}",
                    from, batch_id
                );
                return;
            }
        };
        let commitment = match commitments
            .iter()
            .find(|x| x.0 == *from && x.1.ticket.hash() == hash)
        {
            Some(commitment) => &commitment.1,
            None => {
                info!("No commitment for payload from {}", from);
                return;
            }
        };

        // TODO: verify payload matches commitment
        // TODO: verify shape of distro_results

        // we only care to add this to consensus & track it in batch IDs if we have any batch IDs that haven't yet been voted for.
        // TODO: how do we do witnessing for verifiers that might be training on data that's not in the normal remaining batch IDs?
        // TODO: also we want ALL those from everyone, right?
        let just_finished = if let Some((num_batch_ids_left, batch_ids_not_yet_trained_on)) =
            &mut round_state.batch_ids_not_yet_trained_on
        {
            let mut remaining_batch_ids = batch_ids_not_yet_trained_on.lock().await;

            match round_state.blooms.as_mut() {
                Some((participant_bloom, batch_bloom)) => {
                    participant_bloom.add(&sha256(from.as_ref()));
                    if remaining_batch_ids.contains(batch_id) {
                        // first received payload for this batch id, vote for it in consensus
                        batch_bloom.add(&sha256(&commitment.commitment));
                        debug!("Adding batch {batch_id} to participant bloom");
                    } else {
                        debug!(
                            "Don't have {} in our remaining batch IDs {:?}, discarding",
                            batch_id, remaining_batch_ids
                        );
                    }
                }
                None => {
                    debug!(
                        "Already submitted witness, not adding {} to participant bloom",
                        from
                    );
                }
            }

            remaining_batch_ids.remove(batch_id);

            debug!(
                "Remaining batches to download for step {}: {:?}",
                distro_result.step, remaining_batch_ids
            );

            *num_batch_ids_left = remaining_batch_ids.len();

            remaining_batch_ids.is_empty()
        } else {
            debug!("All batches already trained on, discarding batch {batch_id}");
            false
        };

        if just_finished {
            round_state.batch_ids_not_yet_trained_on = None;
            self.notify_try_opportunistic_witness.notify_one();
        }

        // we unconditionally store every seen payload, since we're not yet sure what consensus will be on whether it's included.
        let deserializing = tokio::task::spawn({
            let notify_try_opportunistic_witness = self.notify_try_opportunistic_witness.clone();
            let batch_id = *batch_id;
            async move {
                let maybe_results = tokio::task::spawn_blocking(move || {
                    let r = distro_result
                        .distro_results
                        .iter()
                        .map(|x| x.try_into())
                        .collect::<Result<Vec<DistroResult>, TchError>>();
                    debug!(
                        "Finished deserializing payload {} for batch {}",
                        hash, batch_id
                    );
                    r
                })
                .await
                .map_err(|_| DeserializeError::DeserializeThreadCrashed)??;
                notify_try_opportunistic_witness.notify_one();
                Ok(maybe_results)
            }
        });

        round_state
            .downloads
            .insert(hash, PayloadState::Deserializing(deserializing));
    }

    async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), StepError> {
        let client_index = match state
            .epoch_state
            .clients
            .iter()
            .position(|x| x.id == self.identity)
        {
            Some(index) => index as u64,
            None => {
                trace!(
                    "saw new step, but we're not one of the clients. our id: {}, all clients: {:?}",
                    self.identity,
                    &state
                        .epoch_state
                        .clients
                        .iter()
                        .map(|c| c.id)
                        .collect::<Vec<_>>()
                );
                let new_step = match std::mem::take(&mut self.active_step) {
                    ActiveStep::Intermediate => {
                        unreachable!("can never be in intermediate state.")
                    }
                    ActiveStep::Warmup(warmup) => ActiveStep::Warmup(warmup),
                    ActiveStep::Cooldown(cooldown) => {
                        trace!("since we're not a member of this step, killing cooldown step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            cooldown.finish().await?,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                    ActiveStep::Training(training) => {
                        trace!("since we're not a member of this step, killing training step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            training.finish().await?.evals_or_trainers,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                    ActiveStep::Witness(witness) => {
                        trace!("since we're not a member of this step, killing witness step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(
                            witness.finish().await?,
                            &mut self.previous_round,
                            &mut self.current_round,
                        ))
                    }
                };
                self.active_step = new_step;

                return Ok(());
            }
        };

        let new_step: ActiveStep = match (std::mem::take(&mut self.active_step), state.run_state) {
            // start training at the beginning of an epoch
            (ActiveStep::Warmup(warmup), RunState::RoundTrain) => {
                let trainers = warmup.finish().stop_evals().await?;
                self.stats_logger.push_eval_results();
                ActiveStep::Training(self.training.start(
                    client_index,
                    &state,
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                    self.notify_try_opportunistic_witness.clone(),
                )?)
            }

            // start witnessing after training is done
            (ActiveStep::Training(training), RunState::RoundWitness) => {
                let FinishedTrainers {
                    evals_or_trainers,
                    round_losses,
                    optim_stats,
                    round_duration,
                } = training.finish().await?;
                let loss =
                    self.stats_logger
                        .push_round_stats(&round_losses, round_duration, optim_stats);
                info!(
                    client_id = %self.identity,
                    epoch = state.progress.epoch,
                    step = state.progress.step,
                    loss = loss,
                    "client_loss",
                );
                self.stats_logger
                    .publish_round_stats(&state, &self.node_info);
                ActiveStep::Witness(self.witness.start(
                    client_index,
                    &state,
                    evals_or_trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                )?)
            }
            // within an epoch, loop back to training after witnessing
            (ActiveStep::Witness(witnessing), RunState::RoundTrain) => {
                let trainers = witnessing.finish().await?.stop_evals().await?;
                ActiveStep::Training(self.training.start(
                    client_index,
                    &state,
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                    self.notify_try_opportunistic_witness.clone(),
                )?)
            }

            // the epoch ended & we're transitioning to cooldown
            (ActiveStep::Witness(witnessing), RunState::Cooldown) => {
                let trainers = witnessing.finish().await?.stop_evals().await?;
                ActiveStep::Cooldown(self.cooldown.start(trainers, &state)?)
            }
            // cooldown is done, we consider waiting for members and warmup to be basically the same
            (ActiveStep::Cooldown(cooldown), RunState::WaitingForMembers)
            | (ActiveStep::Cooldown(cooldown), RunState::Warmup) => {
                let trainers = cooldown.finish().await?;
                ActiveStep::Warmup(self.warmup.start(
                    trainers,
                    &mut self.previous_round,
                    &mut self.current_round,
                ))
            }
            // stay in existing run state if there's no reason to change.
            (current_step, next_run_state) if current_step.allowed_in_run_state(next_run_state) => {
                current_step
            }
            // but if it's not allowed in this run state, we've desynced.
            (current_step, next_run_state) => {
                let step_error = StepError::Desync {
                    active_step: current_step.to_string(),
                    run_state: next_run_state,
                };
                debug!("DESYNC: {step_error}");
                return Err(step_error);
            }
        };
        self.active_step = new_step;
        self.coordinator_state = state;

        Ok(())
    }

    pub fn set_node_info(&mut self, node_info: HashMap<String, DataValue>) {
        self.node_info = node_info;
    }
}

#[derive(Default, Debug)]
enum ActiveStep {
    #[default]
    Intermediate,

    Warmup(WarmupStep),
    Training(TrainingStep),
    Witness(WitnessStep),
    Cooldown(CooldownStep),
}

impl ActiveStep {
    pub fn allowed_in_run_state(&self, run_state: RunState) -> bool {
        match (self, run_state) {
            (ActiveStep::Intermediate, _) => {
                unreachable!("the intermediate run state can never be seen, it's ephemeral")
            }
            (ActiveStep::Warmup(..), RunState::Warmup | RunState::WaitingForMembers) => true,
            (ActiveStep::Cooldown(..), RunState::Cooldown) => true,
            (ActiveStep::Training(..), RunState::RoundTrain) => true,
            (ActiveStep::Witness(..), RunState::RoundWitness) => true,
            _ => false,
        }
    }
}

impl fmt::Display for ActiveStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveStep::Intermediate => write!(f, "Intermediate"),
            ActiveStep::Warmup(_) => write!(f, "Warmup"),
            ActiveStep::Training(_) => write!(f, "Training"),
            ActiveStep::Witness(_) => write!(f, "Witness"),
            ActiveStep::Cooldown(_) => write!(f, "Cooldown"),
        }
    }
}

pub enum InitStage<T: NodeIdentity, A: AuthenticatableIdentity> {
    NotYetInitialized(Option<RunInitConfigAndIO<T, A>>),
    #[allow(clippy::type_complexity)]
    Initializing(JoinHandle<Result<StepStateMachine<T, A>, InitRunError>>),
    Running(StepStateMachine<T, A>),
}

pub struct RunManager<T: NodeIdentity, A: AuthenticatableIdentity>(InitStage<T, A>);

#[derive(Error, Debug)]
pub enum ApplyStateError {
    #[error("Failed to init run in warmup: {0}")]
    Init(InitRunError),

    #[error("Failed to run step: {0}")]
    Step(#[from] StepError),
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> RunManager<T, A> {
    pub fn new(config: RunInitConfigAndIO<T, A>) -> Self {
        Self(InitStage::NotYetInitialized(Some(config)))
    }

    /// # async safety:
    /// this will wait forever if not running - you must use this in a select! that can also apply a new state.
    pub async fn opportunistic_witness_wait_notified(&mut self) {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .notify_try_opportunistic_witness
                    .notified()
                    .await;
            }
            _ => {
                // wait forever - this will get pre-empted by a state change in select that moves us to a running stage.
                std::future::pending().await
            }
        }
    }

    pub async fn try_send_opportunistic_witness(
        &mut self,
    ) -> Result<(), OpportunisticWitnessError> {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine.try_send_opportunistic_witness().await?;
            }
            _ => {
                panic!("Implementation error: you should never call this until `opportunistic_witness_try_ready` resolves.")
            }
        }
        Ok(())
    }

    pub async fn apply_message(
        &mut self,
        from_client_id: T,
        training_result: TrainingResult,
    ) -> Result<(), ApplyMessageError> {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .apply_message(from_client_id, training_result)
                    .await
            }
            _ => {
                // not yet warmed up, ignore any p2p messages.
                Ok(())
            }
        }
    }

    pub async fn apply_distro_result(
        &mut self,
        hash: psyche_network::Hash,
        distro_result: TransmittableDistroResult,
        self_result: Option<Vec<DistroResult>>,
    ) {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .apply_distro_result(hash, distro_result, self_result)
                    .await;
            }
            _ => {
                // not yet warmed up, ignore any p2p messages.
            }
        }
    }

    pub async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), ApplyStateError> {
        let new_state = match &mut self.0 {
            InitStage::NotYetInitialized(init_info @ Some(..))
                if state.run_state == RunState::Warmup =>
            {
                // Take ownership of init_info using std::mem::take
                let init_info = init_info.take().unwrap();
                Some(InitStage::Initializing(tokio::spawn(
                    init_info.init_run(state),
                )))
            }
            InitStage::NotYetInitialized(None) => {
                unreachable!("Once we take the init state, we move to initializing.");
            }
            InitStage::Initializing(..) if state.run_state == RunState::WaitingForMembers => {
                // a client has left the network, transitioning back to RunState::WaitingForMembers.
                // wait for new clients to join the network.
                return Ok(());
            }
            InitStage::Initializing(ref mut init_future) => {
                // Try to complete initialization
                match init_future.is_finished() {
                    true => match init_future.await.unwrap() {
                        Ok(state_machine) => Some(InitStage::Running(state_machine)),
                        Err(e) => {
                            return Err(ApplyStateError::Init(e));
                        }
                    },
                    false => {
                        // We're still initializing, keep current state
                        return Ok(());
                    }
                }
            }
            // we're running, process it in a sec
            InitStage::Running(..) => None,
            // not initialized but we haven't seen a warmup yet, we're just waiting!
            InitStage::NotYetInitialized(_) => {
                return Ok(());
            }
        };

        if let Some(new_state) = new_state {
            self.0 = new_state;
        }

        // yay ok new state! let's go!
        if let InitStage::Running(state_machine) = &mut self.0 {
            state_machine
                .apply_state(state)
                .instrument(info_span!("StepStateMachine::apply_state"))
                .await?;
        }

        Ok(())
    }

    pub fn stats(&self) -> Option<&StatsLogger> {
        match &self.0 {
            InitStage::Running(run) => Some(&run.stats_logger),
            _ => None,
        }
    }

    pub fn set_node_info(&mut self, node_info: HashMap<String, DataValue>) {
        if let InitStage::Running(run) = &mut self.0 {
            run.set_node_info(node_info)
        }
    }
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> From<&RunManager<T, A>>
    for ClientTUIState
{
    fn from(run: &RunManager<T, A>) -> Self {
        match &run.0 {
            InitStage::Running(state_machine) => {
                let coordinator = &state_machine.coordinator_state;
                let committee = state_machine
                    .current_round
                    .committee_info
                    .as_ref()
                    .map(|x| x.0.committee);
                let stats = run.stats();

                ClientTUIState {
                    step: coordinator.progress.step,
                    committee,
                    run_state: coordinator.into(),
                    loss: stats.map(|s| s.losses().to_vec()).unwrap_or_default(),
                    batches_left: state_machine
                        .current_round
                        .batch_ids_not_yet_trained_on
                        .as_ref()
                        .map(|x| x.0)
                        .unwrap_or_default(),
                    global_tokens_per_second: stats
                        .map(|s| s.global_tokens_per_second(coordinator))
                        .unwrap_or_default(),
                    total_tokens: coordinator.total_tokens(),
                    evals: stats.map(|s| s.eval_history().clone()).unwrap_or_default(),
                }
            }
            _ => Default::default(),
        }
    }
}
