use std::{collections::HashMap, fmt, future::Future, pin::Pin};

use psyche_coordinator::{Committee, Coordinator, RunState, Witness};
use psyche_core::{sha256, BatchId};
use psyche_modeling::DistroResult;
use psyche_network::{BlobTicket, Hash, NetworkableNodeIdentity};
use tch::TchError;
use thiserror::Error;
use tokio::sync::mpsc::{self};
use tracing::{debug, error, info, info_span, trace, warn, Instrument};
use wandb::DataValue;

use crate::{
    state::{train::FinishedTrainers, types::DeserializeError},
    trainer::Trainer,
    ClientTUIState, TrainingResult, TransmittableDistroResult,
};

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

pub struct StepStateMachine<T: NetworkableNodeIdentity> {
    identity: T,

    stats_logger: StatsLogger,

    warmup: WarmupStepMetadata,
    training: TrainingStepMetadata<T>,
    witness: WitnessStepMetadata<T>,
    cooldown: CooldownStepMetadata,

    active_step: ActiveStep,

    tx_request_download: mpsc::Sender<BlobTicket>,
    tx_witness: mpsc::Sender<Witness>,

    tx_try_opportunistic_witness: mpsc::Sender<()>,
    rx_try_opportunistic_witness: mpsc::Receiver<()>,

    current_round: RoundState<T>,
    previous_round: RoundState<T>,

    coordinator_state: Coordinator<T>,

    // don't use me for real logic, this is only used for the TUI.
    _num_batches_left_to_train_on_this_round: usize,

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
pub enum ApplyDistroResultError {
    #[error("Failed to queue opportinistic witness check")]
    TryOpportunisticWitness,
}

#[derive(Error, Debug)]
pub enum OpportunisticWitnessError {
    #[error("Failed to send opportunistic witness, channel must be closed")]
    Send,
}

impl<T: NetworkableNodeIdentity> StepStateMachine<T> {
    pub fn new(
        identity: T,
        warmup: WarmupStepMetadata,
        training: TrainingStepMetadata<T>,
        witness: WitnessStepMetadata<T>,
        cooldown: CooldownStepMetadata,
        trainers: Vec<Trainer>,
        coordinator_state: Coordinator<T>,
        tx_request_download: mpsc::Sender<BlobTicket>,
        tx_witness: mpsc::Sender<Witness>,
        stats_logger: StatsLogger,
    ) -> Self {
        let active_step = ActiveStep::Warmup(warmup.start(trainers));
        let (tx_try_opportunistic_witness, rx_try_opportunistic_witness) = mpsc::channel(10);
        Self {
            identity,

            stats_logger,

            warmup,
            training,
            witness,
            cooldown,
            active_step,

            current_round: RoundState::new(),
            previous_round: RoundState::new(),

            tx_request_download,
            tx_witness,
            tx_try_opportunistic_witness,
            rx_try_opportunistic_witness,

            coordinator_state,

            _num_batches_left_to_train_on_this_round: 0,
            node_info: HashMap::new(),
        }
    }

    pub async fn try_send_opportunistic_witness(
        &mut self,
    ) -> Result<(), OpportunisticWitnessError> {
        let prev_round = self.coordinator_state.overlapped && !self.coordinator_state.first_round;
        let opportunistic_witness_round = match prev_round {
            true => &mut self.previous_round,
            false => &mut self.current_round,
        };
        if !matches!(self.active_step, ActiveStep::Training(..)) {
            // nothin to do, we're not training, so there's no reason to send an opportunistic witness.
            return Ok(());
        }

        if let Some((_, witness_proof, _)) = opportunistic_witness_round.committee_info {
            // if we're overlapped & on the first two rounds, we'll send a witness proof,
            // even though it's not going to be full -
            // this is so we don't stall on the first round, and wait the entire training round time.
            // TODO maybe explicitly encode this in the coordinator state, so there's no weirdness about
            // signing off on a weird witness proof?
            let skip_ready_check =
                self.coordinator_state.overlapped && opportunistic_witness_round.height <= 1;
            if !skip_ready_check {
                // check that we've seen a payload for every batch ID
                if opportunistic_witness_round
                    .batch_ids_not_yet_trained_on
                    .is_some()
                {
                    // we're not done training yet, still some batch IDs to recv payloads for.
                    return Ok(());
                }
                // check that all batches are done deserializing
                for batch in &opportunistic_witness_round.downloads {
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
            }

            debug!(
                "Sending opportunistic witness for {} round. skipped ready check? {}",
                if prev_round { "previous" } else { "current" },
                skip_ready_check
            );

            if let Some(witness) =
                opportunistic_witness_round.get_witness_to_send(witness_proof.index)
            {
                self.tx_witness
                    .send(witness)
                    .await
                    .map_err(|_| OpportunisticWitnessError::Send)?;
            }
        }
        Ok(())
    }

    pub async fn apply_message(
        &mut self,
        from_client_id: T,
        training_result: TrainingResult,
    ) -> Result<(), ApplyMessageError> {
        let state_step = self.coordinator_state.step;
        let result_step = training_result.step;
        let batch_id = training_result.batch_id;
        let round_state = if self.coordinator_state.overlapped {
            if training_result.step == state_step {
                debug!(
                    "Got result gossip for current step {} batch {batch_id}",
                    result_step
                );
                &mut self.current_round
            } else if result_step == state_step - 1 {
                debug!(
                    "Got result gossip for previous step {} batch {batch_id}",
                    result_step
                );
                &mut self.previous_round
            } else {
                debug!(
                    "Ignoring result from step {} (current step is {})",
                    result_step, state_step
                );
                return Ok(());
            }
        } else if training_result.step != state_step {
            debug!(
                "Ignoring result from step {} (current step is {})",
                training_result.step, state_step
            );
            return Ok(());
        } else {
            debug!(
                "Got result gossip for current step {} batch {batch_id}",
                result_step
            );
            &mut self.current_round
        };

        let check_committee = from_client_id != self.identity;
        if check_committee {
            match &round_state.committee_info {
                Some((_, _, committee_info)) => {
                    if !committee_info.verify_committee_for_client(
                        &from_client_id,
                        &training_result.proof,
                        &self.coordinator_state.clients,
                    ) {
                        debug!("Committee verification failed for commitment 0x{} (step={},batch_id={}) received from {}", hex::encode(training_result.commitment),                              training_result.step,
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
            return Ok(());
        }

        let client_commitments = *round_state
            .commitments_per_client
            .get(&from_client_id)
            .unwrap_or(&0);

        let first_data_id = BatchId::from_u64(
            u64::from(training_result.batch_id)
                * self.coordinator_state.data_indicies_per_batch as u64,
        );
        let correct_assignee = match round_state.data_assignments.get(first_data_id) {
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
            .insert(from_client_id.clone(), client_commitments + 1);

        let total_commitments = round_state
            .commitments_per_client
            .values()
            .fold(0, |acc, ele| acc + *ele);

        debug!(
            "Total commitments for step {}: {}",
            self.coordinator_state.step, total_commitments
        );

        if let Some((_, witness_proof, _)) = round_state.committee_info.as_ref() {
            if witness_proof.witness {
                if let Some((commit_bloom, participant_bloom, order_bloom)) =
                    &mut round_state.blooms
                {
                    commit_bloom.add(&sha256(&training_result.commitment));
                    participant_bloom.add(&sha256(from_client_id.as_ref()));
                    // Note: need to check if this batch_id is in remaining_batch_ids for order_bloom
                    order_bloom.add(&sha256(&training_result.commitment));
                }
            }
        }

        round_state
            .results
            .entry(training_result.batch_id)
            .or_default();
        let batch_id = training_result.batch_id;
        let step = training_result.step;
        round_state
            .results
            .get_mut(&training_result.batch_id)
            .unwrap()
            .push((from_client_id.clone(), training_result));
        let download_state =
            PayloadState::Downloading((from_client_id.clone(), batch_id, ticket.clone()));
        round_state.downloads.insert(hash, download_state);

        // start downloading the payload unless this is a self-message
        // (assuming the caller will put our payload in the proper place)
        if from_client_id != self.identity {
            debug!(
                "Requesting download of step {} batch {}: {}",
                step,
                batch_id,
                ticket.hash()
            );

            self.tx_request_download
                .send(ticket)
                .await
                .map_err(|_| ApplyMessageError::StartDownloadBlob)?;
        }

        Ok(())
    }

    pub async fn apply_distro_result(
        &mut self,
        hash: Hash,
        distro_result: TransmittableDistroResult,
    ) -> Result<(), ApplyDistroResultError> {
        let round_state = if self.current_round.downloads.contains_key(&hash) {
            &mut self.current_round
        } else if self.previous_round.downloads.contains_key(&hash) {
            &mut self.previous_round
        } else {
            debug!("Unknown download {}", hash);
            return Ok(());
        };

        debug!(
            "Finished download of distro result for batch {} in step {} with hash {hash}",
            distro_result.batch_id, distro_result.step
        );

        let (from, batch_id, _) = match round_state.downloads.get(&hash) {
            Some(PayloadState::Downloading(x)) => x,
            Some(PayloadState::Deserializing(_)) => {
                debug!("Duplicate download of {}", hash);
                return Ok(());
            }
            None => {
                debug!("Unknown download {}", hash);
                return Ok(());
            }
        };

        let commitments = match round_state.results.get(batch_id) {
            Some(commitments) => commitments,
            None => {
                info!(
                    "No commitment for payload from {} for batch {}",
                    from, batch_id
                );
                return Ok(());
            }
        };
        let commitment = match commitments
            .iter()
            .find(|x| x.0 == *from && x.1.ticket.hash() == hash)
        {
            Some(commitment) => &commitment.1,
            None => {
                info!("No commitment for payload from {}", from);
                return Ok(());
            }
        };

        let witness_proof = match round_state.committee_info.as_ref() {
            Some((_, witness_proof, _)) => witness_proof,
            None => {
                warn!("Got a commitment for a round without a committee. wtf?");
                return Ok(());
            }
        };
        // TODO: verify payload matches commitment
        // TODO: verify shape of distro_results

        // we only care to add this to consensus & track it in batch IDs if we have any batch IDs that haven't yet been voted for.
        // TODO: how do we do witnessing for verifiers that might be training on data that's not in the normal remaining batch IDs?
        // TODO: also we want ALL those from everyone, right?
        let just_finished = if let Some(batch_ids_not_yet_trained_on) =
            &mut round_state.batch_ids_not_yet_trained_on
        {
            let mut remaining_batch_ids = batch_ids_not_yet_trained_on.lock().await;
            if witness_proof.witness {
                match round_state.blooms.as_mut() {
                    Some((_, participant_bloom, order_bloom)) => {
                        participant_bloom.add(&sha256(from.as_ref()));
                        if remaining_batch_ids.contains(batch_id) {
                            // first received payload for this batch id, vote for it in consensus
                            order_bloom.add(&sha256(&commitment.commitment));
                            debug!("Adding batch {batch_id} to participant bloom");
                        } else {
                            debug!("Don't have {batch_id} in our remaining batch IDs, discarding");
                        }
                    }
                    None => {
                        debug!(
                            "Already submitted witness, not adding {} to participant bloom",
                            from
                        );
                    }
                }
            } else {
                trace!("not a witness, no bloom to add to.");
            }

            remaining_batch_ids.remove(batch_id);

            debug!(
                "Remaining batches to download for step {}: {:?}",
                distro_result.step, remaining_batch_ids
            );
            remaining_batch_ids.is_empty()
        } else {
            debug!("All batches already trained on, discarding batch {batch_id}");
            false
        };

        if just_finished {
            round_state.batch_ids_not_yet_trained_on = None;
            self.tx_try_opportunistic_witness
                .send(())
                .await
                .map_err(|_| ApplyDistroResultError::TryOpportunisticWitness)?;
        }

        // we unconditionally store every seen payload, since we're not yet sure what consensus will be on whether it's included.
        let deserializing = tokio::task::spawn({
            let tx_try_witness = self.tx_try_opportunistic_witness.clone();
            async move {
                let maybe_results = tokio::task::spawn_blocking(move || {
                    distro_result
                        .distro_results
                        .iter()
                        .map(|x| x.try_into())
                        .collect::<Result<Vec<DistroResult>, TchError>>()
                })
                .await
                .map_err(|_| DeserializeError::DeserializeThreadCrashed)??;
                tx_try_witness
                    .send(())
                    .await
                    .map_err(|_| DeserializeError::NotifyDone)?;
                Ok(maybe_results)
            }
        });

        round_state
            .downloads
            .insert(hash, PayloadState::Deserializing(deserializing));

        Ok(())
    }

    async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), StepError> {
        let client_index = match state.clients.iter().position(|x| x.id == self.identity) {
            Some(index) => index as u64,
            None => {
                trace!(
                    "saw new step, but we're not one of the clients. our id: {}, all clients: {:?}",
                    self.identity,
                    &state
                        .clients
                        .iter()
                        .map(|c| c.id.clone())
                        .collect::<Vec<_>>()
                );
                let new_step = match std::mem::take(&mut self.active_step) {
                    ActiveStep::Intermediate => {
                        unreachable!("can never be in intermediate state.")
                    }
                    ActiveStep::Warmup(warmup) => ActiveStep::Warmup(warmup),
                    ActiveStep::Cooldown(cooldown) => {
                        trace!("since we're not a member of this step, killing cooldown step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(cooldown.finish().await?))
                    }
                    ActiveStep::Training(training) => {
                        trace!("since we're not a member of this step, killing training step and returning to warmup to wait.");
                        ActiveStep::Warmup(
                            self.warmup
                                .start(training.finish().await?.evals_or_trainers),
                        )
                    }
                    ActiveStep::Witness(witness) => {
                        trace!("since we're not a member of this step, killing witness step and returning to warmup to wait.");
                        ActiveStep::Warmup(self.warmup.start(witness.finish().await?))
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
                )?)
            }

            // start witnessing after training is done
            (ActiveStep::Training(training), RunState::RoundWitness) => {
                let FinishedTrainers {
                    evals_or_trainers,
                    round_losses,
                    optim_stats,
                } = training.finish().await?;
                let loss = self
                    .stats_logger
                    .push_round_stats(&round_losses, optim_stats);
                info!("Step {} loss: {}", state.step, loss);
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
                )?)
            }

            // the epoch ended & we're transitioning to cooldown
            (ActiveStep::Witness(witnessing), RunState::Cooldown) => {
                let trainers = witnessing.finish().await?.stop_evals().await?;
                ActiveStep::Cooldown(self.cooldown.start(trainers, &state)?)
            }
            // cooldown is done, we consider waiting for members and warmup to be basically the same
            (ActiveStep::Cooldown(cooldown), RunState::WaitingForMembers) => {
                let trainers = cooldown.finish().await?;
                ActiveStep::Warmup(self.warmup.start(trainers))
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

pub enum InitStage<T: NetworkableNodeIdentity> {
    NotYetInitialized(Option<RunInitConfigAndIO<T>>),
    Initializing(Pin<Box<dyn Future<Output = Result<StepStateMachine<T>, InitRunError>> + Send>>),
    Running(StepStateMachine<T>),
}

pub struct RunManager<T: NetworkableNodeIdentity>(InitStage<T>);

#[derive(Error, Debug)]
pub enum ApplyStateError {
    #[error("Failed to init run in warmup: {0}")]
    Init(InitRunError),

    #[error("Failed to run step: {0}")]
    Step(#[from] StepError),
}

impl<T: NetworkableNodeIdentity> RunManager<T> {
    pub fn new(config: RunInitConfigAndIO<T>) -> Self {
        Self(InitStage::NotYetInitialized(Some(config)))
    }

    /// # async safety:
    /// this will wait forever if not running - you must use this in a select! that can also apply a new state.
    pub async fn opportunistic_witness_try_ready(&mut self) -> Option<()> {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine.rx_try_opportunistic_witness.recv().await
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
    ) -> Result<(), ApplyDistroResultError> {
        match &mut self.0 {
            InitStage::Running(state_machine) => {
                state_machine
                    .apply_distro_result(hash, distro_result)
                    .await?;
            }
            _ => {
                // not yet warmed up, ignore any p2p messages.
            }
        }

        Ok(())
    }

    pub async fn apply_state(&mut self, state: Coordinator<T>) -> Result<(), ApplyStateError> {
        let new_state = match &mut self.0 {
            InitStage::NotYetInitialized(init_info @ Some(..))
                if state.run_state == RunState::Warmup =>
            {
                // Take ownership of init_info using std::mem::take
                let init_info = init_info.take().unwrap();
                Some(InitStage::Initializing(Box::pin(
                    init_info.init_run(state.clone()),
                )))
            }
            InitStage::NotYetInitialized(None) => {
                unreachable!("Once we take the init state, we move to initializing.");
            }
            InitStage::Initializing(..) if state.run_state != RunState::Warmup => {
                unimplemented!(
                    "we missed warmup while warming up! abort. maybe handle this gracefully?"
                )
            }
            InitStage::Initializing(ref mut init_future) => {
                // Try to complete initialization
                match futures::poll!(init_future) {
                    std::task::Poll::Ready(Ok(state_machine)) => {
                        Some(InitStage::Running(state_machine))
                    }
                    std::task::Poll::Ready(Err(e)) => {
                        return Err(ApplyStateError::Init(e));
                    }
                    std::task::Poll::Pending => {
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

impl<T: NetworkableNodeIdentity> From<&RunManager<T>> for ClientTUIState {
    fn from(run: &RunManager<T>) -> Self {
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
                    step: coordinator.step,
                    committee,
                    run_state: coordinator.into(),
                    loss: stats.map(|s| s.losses().to_vec()).unwrap_or_default(),
                    batches_left: state_machine._num_batches_left_to_train_on_this_round,
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
