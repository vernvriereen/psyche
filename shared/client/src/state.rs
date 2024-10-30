use crate::{
    disto_results_to_bytes,
    fetch_data::{Batch, BatchId, BatchStep, DataFetcher, TrainingDataForStep},
    protocol::TrainingResult,
    trainer::{DistroResults, ParallelModels, TrainOutput, Trainer},
    tui::ClientTUIState,
    BroadcastMessage, Payload, PeerAnnouncement, SerializedDistroResult, WandBInfo,
};
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    assign_data_for_state, get_batch_ids_for_round, model, Committee, CommitteeProof,
    CommitteeSelection, Coordinator, HealthChecks, RunState, Witness, WitnessProof,
    BLOOM_FALSE_RATE, BLOOM_MAX_BITS, NUM_STORED_ROUNDS,
};
use psyche_core::{sha256, Bloom, BoundedQueue, IntervalTree, NodeIdentity, RunningAverage};
use psyche_data_provider::{
    download_model_repo_async, upload_model_repo_async, DataProviderTcpClient,
};
use psyche_modeling::{
    auto_tokenizer, save_tensors_into_safetensors, CommunicatorId, DistroResult, LlamaForCausalLM,
};
use psyche_network::{dummy_blob_ticket, BlobTicket, NetworkEvent};
use psyche_watcher::{Backend, BackendWatcher};
use rand::{seq::SliceRandom, thread_rng, RngCore};
use std::{
    collections::HashMap,
    fs,
    ops::Deref,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tch::{Device, Kind};
use tokenizers::Tokenizer;
use tokio::{runtime::Handle, select, sync::Notify, task::JoinHandle, time::sleep};
use tracing::{debug, error, info, warn};
use wandb::LogData;

const WARMUP_PEER_ANNOUNCEMENT_DURATION: Duration = Duration::from_secs(10);
const DOWNLOAD_RETRIES: usize = 3;

type TaskResult<T> = Option<JoinHandle<Result<T>>>;

enum PayloadState<T: NodeIdentity> {
    Downloading((T, u64, BlobTicket)),
    Deserializing(JoinHandle<Result<Vec<DistroResult>>>),
}

pub type BroadcastAndPayload = (BroadcastMessage, Payload);

pub enum ToSend {
    Nothing,
    Broadcast(BroadcastAndPayload),
    Witness(Witness),
    HealthCheck(HealthChecks),
    Checkpoint(model::Checkpoint),
}

type Bloom32 = Bloom<[u8; 32]>;

type Rollbacks = BoundedQueue<(BatchStep, Vec<DistroResults>)>;

struct EvalTask {
    task: psyche_eval::PreparedTask,
    results: Arc<RunningAverage>,
    next_index: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub struct CheckpointUploadInfo {
    pub hub_repo: String,
    pub hub_token: String,
    pub checkpoint_dir: PathBuf,
}

pub struct State<T: NodeIdentity> {
    pub identity: T,
    private_key: T::PrivateKey,
    showed_inclusion_message: bool,
    data_and_model_load: TaskResult<LoadedModelAndData<T>>,
    available_trainers: Vec<Trainer>,
    trainings: Vec<JoinHandle<Result<(TrainOutput, u64)>>>,
    applying: TaskResult<Vec<Trainer>>,
    health_checking: TaskResult<HealthChecks>,
    committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    commitments: HashMap<u64, Vec<(T, TrainingResult)>>,
    prev_commitments: HashMap<u64, Vec<(T, TrainingResult)>>,
    commitments_per_client: HashMap<T, u32>,
    payloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    prev_payloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    blooms: Option<(Bloom32, Bloom32, Bloom32)>,
    losses: Vec<f32>,
    round_losses: Vec<f32>,
    data_parallelism: usize,
    tensor_parallelism: usize,
    notify_train_start: Arc<Notify>,
    micro_batch_size: Option<usize>,
    write_gradients_dir: Option<PathBuf>,
    atomic_run_state: Arc<AtomicUsize>,
    round_rollbacks: Arc<tokio::sync::Mutex<Rollbacks>>,
    training_data: Option<TrainingDataForStep>,
    data_fetcher: Option<DataFetcher<T>>,
    round_start: Option<Instant>,
    round_durations: BoundedQueue<Duration>,
    data_assignments: IntervalTree<u64, T>,
    eval_cancel: Arc<AtomicBool>,
    eval_tasks: Vec<psyche_eval::Task>,
    eval_task_max_docs: Option<usize>,
    prepared_eval_tasks: Vec<Arc<EvalTask>>,
    preparing_eval_tasks: TaskResult<Vec<psyche_eval::PreparedTask>>,
    evals: Vec<JoinHandle<Result<Trainer>>>,
    tokenizer: Option<Arc<Tokenizer>>,
    apply_start: Option<Instant>,
    started_early_evals: bool,
    checkpoint_extra_files: Vec<PathBuf>,
    checkpointing: TaskResult<(Trainer, Option<model::HubRepo>)>,
    last_warmup_peer_announcement: Option<Instant>,
    checkpoint_upload_info: Option<CheckpointUploadInfo>,
    hub_read_token: Option<String>,
    wandb_info: Option<WandBInfo>,
    wandb_run: Option<Arc<wandb::Run>>,
    wandb_log: LogData,
    retried_downloads: HashMap<psyche_network::Hash, usize>,
    /// only used for the TUI. do not rely upon this staying in sync or i will be very angy >:(
    _last_observed_num_batches_remaining: usize,
    _eval_results: HashMap<String, Vec<f64>>,
}

impl<T: NodeIdentity> State<T> {
    pub fn new(
        identity: T,
        private_key: T::PrivateKey,
        data_parallelism: usize,
        tensor_parallelism: usize,
        eval_tasks: Vec<psyche_eval::Task>,
        eval_task_max_docs: Option<usize>,
        micro_batch_size: Option<usize>,
        write_gradients_dir: Option<PathBuf>,
        checkpoint_upload_info: Option<CheckpointUploadInfo>,
        hub_read_token: Option<String>,
        wandb_info: Option<WandBInfo>,
    ) -> Self {
        assert!(data_parallelism > 0);
        assert!(tensor_parallelism > 0);
        assert!(micro_batch_size.map(|x| x > 0).unwrap_or(true));
        Self {
            identity,
            private_key,
            showed_inclusion_message: false,
            data_and_model_load: None,
            training_data: None,
            available_trainers: Vec::new(),
            trainings: Vec::new(),
            applying: None,
            health_checking: None,
            committee_info: None,
            state: None,
            prev_state: None,
            blooms: None,
            commitments: HashMap::new(),
            prev_commitments: HashMap::new(),
            commitments_per_client: HashMap::new(),
            payloads: HashMap::new(),
            prev_payloads: HashMap::new(),
            losses: Vec::new(),
            round_losses: Vec::new(),
            notify_train_start: Arc::new(Notify::new()),
            data_parallelism,
            tensor_parallelism,
            micro_batch_size,
            write_gradients_dir,
            atomic_run_state: Arc::new(AtomicUsize::new(0)),
            round_rollbacks: tokio::sync::Mutex::new(BoundedQueue::new(NUM_STORED_ROUNDS)).into(),
            data_fetcher: None,
            round_start: None,
            round_durations: BoundedQueue::new(16),
            data_assignments: IntervalTree::new(),
            eval_cancel: Arc::new(AtomicBool::new(false)),
            _eval_results: eval_tasks
                .iter()
                .map(|task| (task.to_string(), Vec::new()))
                .collect(),
            eval_tasks,
            eval_task_max_docs,
            evals: Vec::new(),
            prepared_eval_tasks: Vec::new(),
            preparing_eval_tasks: None,
            tokenizer: None,
            apply_start: None,
            started_early_evals: false,
            last_warmup_peer_announcement: None,
            checkpoint_upload_info,
            hub_read_token,
            checkpoint_extra_files: Vec::new(),
            checkpointing: None,
            wandb_info,
            wandb_run: None,
            wandb_log: LogData::new(),
            retried_downloads: HashMap::new(),
            _last_observed_num_batches_remaining: 0,
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        prev_state: Option<Coordinator<T>>,
    ) -> Result<Option<ToSend>> {
        self.state = Some(state.clone());
        self.prev_state = prev_state;
        let position = match state.clients.iter().position(|x| x.id == self.identity) {
            Some(position) => position as u64,
            None => {
                if !self.showed_inclusion_message {
                    info!("Awaiting inclusion in round");
                    self.showed_inclusion_message = true;
                }
                return Ok(None);
            }
        };
        self.atomic_run_state
            .store(state.run_state.into(), Ordering::Relaxed);
        match state.run_state {
            RunState::WaitingForMembers => Ok(None),
            RunState::Warmup => self.warmup().map(|_| None),
            RunState::RoundTrain => self.round_train(position).await.map(|_| None),
            RunState::RoundWitness => self.round_witness(position).map(|x| x.map(ToSend::Witness)),
            RunState::RoundApply => self.round_apply().await.map(|_| None),
            RunState::Cooldown => self.cooldown().map(|_| None),
        }
    }

    pub fn get_train_start_notification(&self) -> Arc<Notify> {
        self.notify_train_start.clone()
    }

    pub fn log_to_wandb(&mut self, key: String, value: wandb::DataValue) {
        self.wandb_log.insert(key, value);
    }

    fn handle_poll_next_applying(&mut self, trainers: Vec<Trainer>) -> Result<ToSend> {
        self.available_trainers = trainers;
        if let Some(apply_start) = self.apply_start.take() {
            debug!(
                "Apply time: {:.1}s, {} trainers ready",
                (Instant::now() - apply_start).as_secs_f32(),
                self.available_trainers.len()
            );
        }
        Ok(ToSend::Nothing)
    }

    fn handle_poll_next_training_data(
        &mut self,
        batch_id: BatchId,
        batch: Batch,
        batch_step: u32,
    ) -> Result<ToSend> {
        debug!("got data step {batch_step} id: {batch_id}");

        let trainer: Trainer = self
                .available_trainers
                .pop()
                .ok_or(Error::msg("Round start but no trainer object (didn't finish training previous round or applying it?)"))?;

        let round_rollbacks = self.round_rollbacks.clone();
        let handle = Handle::current();
        self.trainings.push(tokio::task::spawn_blocking(move || {
                let rollback: Vec<_> = handle.block_on(async {
                    round_rollbacks
                        .lock()
                        .await
                        .deref()
                        .iter()
                        // we only want to roll back if our state is ahead,
                        // so if we get data for e.g. step 6, but we have rollback data for steps 6, 7, 8,
                        // this will roll back steps 6, 7, 8.
                        .filter(|(from_round, _)| *from_round >= batch_step)
                        .cloned()
                        .collect()
                });
                if !rollback.is_empty() {
                    debug!("Computed rollback - we are training on data for step {batch_step}, so we should roll back steps {}", rollback.iter().map(|f| f.0.to_string()).collect::<Vec<_>>().join(","));
                }

                let output = trainer.train(batch_step, batch, rollback)?;
                Ok((output, batch_id))
            }));
        Ok(ToSend::Nothing)
    }

    fn handle_poll_next_trainings(
        &mut self,
        output: TrainOutput,
        batch_id: BatchId,
    ) -> Result<ToSend> {
        self.available_trainers.push(output.trainer);

        debug!(
            "Batch {} loss: {} cancelled: {}",
            batch_id, output.loss, output.cancelled
        );

        if output.cancelled || !self.is_run_state(RunState::RoundTrain) {
            return Ok(ToSend::Nothing);
        }
        self.round_losses.push(output.loss);
        let (committee_proof, _, _) = self
            .committee_info
            .as_ref()
            .ok_or(Error::msg("Training complete but no self proofs"))?;
        let mut committment = Vec::with_capacity(40);
        committment.extend_from_slice(self.identity.as_ref());
        committment.extend_from_slice(&batch_id.to_be_bytes());
        let commitment = sha256(&committment);
        let step = output.step;
        let broadcast = BroadcastMessage::TrainingResult(TrainingResult {
            step,
            batch_id,
            commitment,
            ticket: dummy_blob_ticket(),
            proof: *committee_proof,
        });
        let payload = Payload::DistroResult(crate::DistroResult {
            step,
            batch_id,
            distro_results: output
                .distro_results
                .iter()
                .map(SerializedDistroResult::try_from)
                .collect::<std::result::Result<Vec<_>, _>>()?,
        });

        // in non-greedy mode we can start evals right when we're done with our work
        if !self.started_early_evals && self.available_trainers.len() == self.data_parallelism {
            if let Some(state) = &self.state {
                if !state.is_greedy_data() {
                    let start = if let Some(training_data) = &self.training_data {
                        // all data has been pushed, we've consumed it all, and all trainers have finished
                        training_data.assigned_ids_done.load(Ordering::SeqCst)
                            && training_data.next_sample.is_empty()
                    } else {
                        // we've already downloaded committments for all batch ids (stronger than just finished our assignments)
                        true
                    };
                    if start {
                        self.started_early_evals = true;
                        self.start_evals();
                    }
                }
            }
        }

        self.maybe_write_gradients(&payload);

        Ok(ToSend::Broadcast((broadcast, payload)))
    }

    fn handle_poll_health_checking(
        &mut self,
        health_checks: Vec<CommitteeProof>,
    ) -> Result<ToSend> {
        match health_checks.is_empty() {
            true => Ok(ToSend::Nothing),
            false => {
                info!(
                    "Sending health check for following indicies: {:?}",
                    health_checks
                );
                Ok(ToSend::HealthCheck(health_checks))
            }
        }
    }

    fn handle_poll_next_preparing_eval_tasks(
        &mut self,
        prepared_tasks: Vec<psyche_eval::PreparedTask>,
    ) -> Result<ToSend> {
        self.prepared_eval_tasks = prepared_tasks
            .into_iter()
            .map(|task| {
                Arc::new(EvalTask {
                    task,
                    results: Arc::new(RunningAverage::new()),
                    next_index: Arc::new(AtomicUsize::new(0)),
                })
            })
            .collect();
        info!("Finished tokenizing eval tasks");
        Ok(ToSend::Nothing)
    }

    fn handle_poll_next_checkpointing(
        &mut self,
        trainer: Trainer,
        hub_repo: Option<model::HubRepo>,
    ) -> Result<ToSend> {
        self.available_trainers.push(trainer);
        match self
            .state
            .as_ref()
            .and_then(|state| state.checkpointers.iter().find(|x| **x == self.identity))
        {
            Some(_) => match hub_repo {
                Some(hub_repo) => Ok(ToSend::Checkpoint(model::Checkpoint::Hub(hub_repo))),
                None => bail!("Checkpointing finished but hub repo not supplied"),
            },
            None => Ok(ToSend::Nothing),
        }
    }

    pub async fn poll_next(&mut self) -> Result<ToSend> {
        let trainings_finished_position = self.trainings.iter_mut().position(|x| x.is_finished());
        if self.is_run_state(RunState::RoundTrain)
            && self.committee_info.is_some()
            && self.training_data.is_none()
        {
            let (_, witness_proof, _) = self.committee_info.as_ref().unwrap();
            if let Some(witness) = self.get_witness_to_send(witness_proof.index) {
                // send opprotunistic witness
                return Ok(ToSend::Witness(witness));
            }
        } else if self.is_run_state(RunState::Warmup) {
            let now = Instant::now();
            if match self.last_warmup_peer_announcement.as_ref() {
                Some(last) => now - *last > WARMUP_PEER_ANNOUNCEMENT_DURATION,
                None => true,
            } {
                self.last_warmup_peer_announcement = Some(now);
                let mut random = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut random);
                return Ok(ToSend::Broadcast((
                    BroadcastMessage::PeerAnnouncement(PeerAnnouncement {
                        ticket: dummy_blob_ticket(),
                    }),
                    Payload::Empty { random },
                )));
            }
        }
        select! {
            applying = async {self.applying.as_mut().unwrap().await}, if self.applying.is_some() => {
                self.applying = None;
                self.handle_poll_next_applying(applying??)
            },
            sample = async {self.training_data.as_mut().unwrap().next_sample.recv().await}, if self.is_run_state(RunState::RoundTrain)
            && !self.available_trainers.is_empty()
            && self.training_data.is_some() => {
                match sample {
                    Some(sample) => self.handle_poll_next_training_data(sample.0, sample.1, self.training_data.as_ref().unwrap().step),
                    None => Ok(ToSend::Nothing),
                }
            },
            finished = async {self.trainings.get_mut(*trainings_finished_position.as_ref().unwrap()).unwrap().await}, if trainings_finished_position.is_some() => {
                self.trainings.swap_remove(trainings_finished_position.unwrap());
                let (output, batch_id) = finished??;
                self.handle_poll_next_trainings(output, batch_id)
            }
            health_checking = async {self.health_checking.as_mut().unwrap().await}, if self.health_checking.is_some() => {
                self.health_checking = None;
                self.handle_poll_health_checking(health_checking??)
            }
            preparing_eval_tasks = async {self.preparing_eval_tasks.as_mut().unwrap().await}, if self.preparing_eval_tasks.is_some() => {
                self.preparing_eval_tasks = None;
                self.handle_poll_next_preparing_eval_tasks(preparing_eval_tasks??)
            }
            checkpointing = async {self.checkpointing.as_mut().unwrap().await}, if self.checkpointing.is_some() => {
                self.checkpointing = None;
                let (trainer, hub_repo) = checkpointing??;
                self.handle_poll_next_checkpointing(trainer, hub_repo)
            }
            _ = sleep(Duration::from_secs_f32(0.1)) => {
                // wakeup to re-evaluate non-waitable conditions
                Ok(ToSend::Nothing)
            }
        }
    }

    pub async fn process_network_event<B: Backend<T> + 'static>(
        &mut self,
        event: NetworkEvent<BroadcastMessage, Payload>,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<Option<BlobTicket>> {
        debug!("Got network event {event:?}");
        match event {
            NetworkEvent::MessageReceived((public_key, message)) => {
                match &message {
                    BroadcastMessage::TrainingResult(training_result) => {
                        // verify they are who they say they are
                        debug!(
                            "Commitment 0x{} (step={},batch_id={}) received from {}",
                            hex::encode(training_result.commitment),
                            training_result.step,
                            training_result.batch_id,
                            public_key
                        );
                        if let Some(state) = &self.state {
                            if state.step == training_result.step {
                                if let Some((_, _, committee_selection)) =
                                    self.committee_info.as_ref()
                                {
                                    if let Some(client) =
                                        watcher.get_client_for_p2p_public_key(public_key.as_bytes())
                                    {
                                        if committee_selection.verify_committee_for_client(
                                            client,
                                            &training_result.proof,
                                            &state.clients,
                                        ) {
                                            return self.handle_broadcast(&client.id, message);
                                        }
                                    }
                                }
                            } else {
                                info!(
                                    "Got broadcast for step {} from {} but current step is {}",
                                    training_result.step, public_key, state.step
                                );
                            }
                        }
                    }
                    BroadcastMessage::PeerAnnouncement(announcement) => {
                        return Ok(Some(announcement.ticket.clone()))
                    }
                }
            }
            NetworkEvent::DownloadComplete(downloaded) => {
                self.retried_downloads.remove(&downloaded.hash);
                match &downloaded.data {
                    Payload::DistroResult(distro_result) => {
                        debug!(
                            "Payload 0x{} received from {}",
                            hex::encode(downloaded.hash),
                            downloaded.from
                        );
                        if let Some(state) = &self.state {
                            if state.step == distro_result.step {
                                self.handle_payload(downloaded.hash, downloaded.data)
                                    .await?;
                            } else {
                                info!(
                                    "Got payload for step {} from {} but current step is {}",
                                    distro_result.step, downloaded.from, state.step
                                );
                            }
                        }
                    }
                    Payload::Empty { random: _ } => {}
                }
            }
            NetworkEvent::DownloadFailed(result) => {
                let retries = *self.retried_downloads.get(&result.hash).unwrap_or(&0);
                if retries >= DOWNLOAD_RETRIES {
                    warn!("Download failed (not retrying): {}", result.error);
                } else {
                    match self.payloads.get(&result.hash) {
                        Some(PayloadState::Downloading((_, _, ticket))) => {
                            info!("Download failed (retrying): {}", result.error);
                            self.retried_downloads
                                .insert(result.hash.clone(), retries + 1);
                            return Ok(Some(ticket.clone()));
                        }
                        _ => {
                            info!("Missing payload for failed download 0x{}", result.hash);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn handle_broadcast(
        &mut self,
        identity: &T,
        broadcast: BroadcastMessage,
    ) -> Result<Option<BlobTicket>> {
        let state = match self.state.as_ref() {
            Some(state) => state,
            None => {
                warn!("Got broadcast but have no state, ignoring");
                return Ok(None);
            }
        };
        let ticket = match broadcast {
            BroadcastMessage::TrainingResult(training_result) => {
                let (_, witness_proof, _) = self
                    .committee_info
                    .as_ref()
                    .ok_or(Error::msg("Broadcast message processor has no self proofs"))?;
                // verified by process_network_event caller
                if training_result.proof.committee == Committee::Trainer {
                    let ticket = training_result.ticket.clone();
                    if self.payloads.contains_key(&ticket.hash()) {
                        // if we already have this payload, ignore
                        return Ok(None);
                    }
                    let client_commitments =
                        *self.commitments_per_client.get(identity).unwrap_or(&0);
                    if state.is_greedy_data() {
                        if client_commitments >= state.max_batches_per_client {
                            debug!(
                                "Maximum commitments received from {}, dropping 0x{}",
                                identity,
                                hex::encode(training_result.commitment)
                            );
                            return Ok(None);
                        }
                    } else {
                        let first_data_id =
                            training_result.batch_id * state.data_indicies_per_batch as u64;
                        let correct_assignee = match self.data_assignments.get(first_data_id) {
                            Some(assignee) => identity == assignee,
                            None => false,
                        };
                        if !correct_assignee {
                            debug!(
                                "Got batch {} from {} but was not assignee, dropping 0x{}",
                                training_result.batch_id,
                                identity,
                                hex::encode(training_result.commitment)
                            );
                            return Ok(None);
                        }
                    }
                    self.commitments_per_client
                        .insert(identity.clone(), client_commitments + 1);
                    let total_commitments = self
                        .commitments_per_client
                        .values()
                        .fold(0, |acc, ele| acc + *ele);
                    info!(
                        "Total commitments for step {}: {}",
                        state.step, total_commitments
                    );

                    if witness_proof.witness {
                        match self.blooms.as_mut() {
                            Some((commit_bloom, _, _)) => {
                                commit_bloom.add(&sha256(&training_result.commitment))
                            }
                            None => {
                                debug!(
                            "Already submitted witness, not adding commitment 0x{} to commit bloom",
                            hex::encode(training_result.commitment)
                        );
                            }
                        }
                    }
                    self.commitments
                        .entry(training_result.batch_id)
                        .or_default();
                    let batch_id = training_result.batch_id;
                    self.commitments
                        .get_mut(&training_result.batch_id)
                        .unwrap()
                        .push((identity.clone(), training_result));
                    self.payloads.insert(
                        ticket.hash(),
                        PayloadState::Downloading((identity.clone(), batch_id, ticket.clone())),
                    );

                    ticket
                } else {
                    // TODO implement broadcast for train / tiebreak
                    error!(
                        "broadcast not implemented for committee member {}",
                        training_result.proof.committee
                    );
                    return Ok(None);
                }
            }
            BroadcastMessage::PeerAnnouncement(peer_announcement) => {
                debug!("Got peer announcement from {identity}");
                peer_announcement.ticket
            }
        };
        // check if this is our broadcast -- if so don't download it (assume caller then calls handle_payload with data)
        match *identity == self.identity {
            true => Ok(None),
            false => Ok(Some(ticket)),
        }
    }

    pub(crate) async fn handle_payload(
        &mut self,
        hash: psyche_network::Hash,
        payload: Payload,
    ) -> Result<()> {
        match payload {
            Payload::DistroResult(distro_result) => {
                let (from, batch_id, _) = match self.payloads.get(&hash) {
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
                let commitments = match self.commitments.get(batch_id) {
                    Some(commitments) => commitments,
                    None => {
                        info!("No commitment for payload from {}", from);
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
                let (_, witness_proof, _) = self
                    .committee_info
                    .as_ref()
                    .ok_or(Error::msg("Payload message processor has no self proofs"))?;
                // TODO: verify payload matches commitment
                // TODO: verify shape of distro_results

                // we only care to add this to consensus & track it in batch IDs if we have any batch IDs that haven't yet been voted for.
                let (just_consumed_last_batch_id, num_left) = if let Some(TrainingDataForStep {
                    batch_ids_not_yet_trained_on,
                    ..
                }) = &mut self.training_data
                {
                    // TODO: how do we do witnessing for verifiers that might be training on data that's not in the normal remaining batch IDs?
                    // TODO: also we want ALL those from everyone, right?
                    let mut remaining_batch_ids = batch_ids_not_yet_trained_on.lock().await; // CANCEL SAFETY
                    if witness_proof.witness {
                        match self.blooms.as_mut() {
                            Some((_, participant_bloom, order_bloom)) => {
                                participant_bloom.add(&sha256(from.as_ref()));
                                if remaining_batch_ids.contains(batch_id) {
                                    // first received payload for this batch id, vote for it in consensus
                                    order_bloom.add(&sha256(&commitment.commitment));
                                }
                            }
                            None => {
                                debug!(
                                    "Already submitted witness, not adding {} to participant bloom",
                                    from
                                );
                            }
                        }
                    }

                    remaining_batch_ids.remove(batch_id);
                    info!(
                        "Remaining batches to download for step {}: {}",
                        distro_result.step,
                        remaining_batch_ids.len()
                    );
                    (remaining_batch_ids.is_empty(), remaining_batch_ids.len())
                } else {
                    // it was already empty, so we didn't just consume the last value.
                    (false, 0)
                };
                self._last_observed_num_batches_remaining = num_left;

                if just_consumed_last_batch_id {
                    self.training_data = None;
                }

                // we unconditionally store every seen payload, since we're not yet sure what consensus will be on whether it's included.
                let deserializing = tokio::task::spawn_blocking(move || {
                    let maybe_results: Result<Vec<DistroResult>, _> = distro_result
                        .distro_results
                        .iter()
                        .map(|x| x.try_into())
                        .collect();
                    maybe_results.map_err(|err| Error::msg(format!("Error deserializing: {}", err)))
                });
                self.payloads
                    .insert(hash, PayloadState::Deserializing(deserializing));
            }
            Payload::Empty { random: _ } => {}
        }
        Ok(())
    }

    fn warmup(&mut self) -> Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in warmup"))?;
        assert_eq!(state.run_state, RunState::Warmup);
        if self.prev_state.is_none()
            || self
                .prev_state
                .as_ref()
                .is_some_and(|x| x.run_state != state.run_state)
        {
            info!("Warming up epoch {}", state.epoch);
            self.round_start = None; // reset throughput metric
            match &state.model {
                Some(model) => {
                    if self.available_trainers.is_empty() {
                        if self.applying.is_none() {
                            self.data_and_model_load =
                                Some(tokio::spawn(State::load_data_and_model(
                                    self.identity.clone(),
                                    self.private_key.clone(),
                                    model.clone(),
                                    self.data_parallelism,
                                    self.tensor_parallelism,
                                    self.hub_read_token.clone(),
                                    self.wandb_info.clone(),
                                )))
                        } else {
                            bail!("Warmup but still applying");
                        }
                    } else {
                        self.start_evals();
                    }
                }
                None => {
                    warn!("Run has no model");
                }
            }
        }
        Ok(())
    }

    async fn round_train(&mut self, index: u64) -> Result<()> {
        if !self.started_early_evals {
            self.cancel_evals().await?; // CANCEL SAFETY
        }

        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round train"))?;
        assert_eq!(state.run_state, RunState::RoundTrain);

        // if all our states are empty (first execution), wait for the data provider and model load to finish
        if self.available_trainers.is_empty()
            && self.data_fetcher.is_none()
            && self.training_data.is_none()
            && self.tokenizer.is_none()
        {
            let data_and_model_load = self.data_and_model_load.take().ok_or(Error::msg(
                "Round started but no model load was running. Did we miss warmup?",
            ))?;
            if !data_and_model_load.is_finished() {
                bail!("Data and model load not finished when round started!")
            }
            let LoadedModelAndData {
                data_provider,
                models,
                tokenizer,
                checkpoint_extra_files,
                wandb_run,
            } = data_and_model_load.await??; // not a cancel safety point, this should return immediately
            self.checkpoint_extra_files = checkpoint_extra_files;
            self.wandb_run = wandb_run.map(Arc::new);

            // TODO add data fetching for verifying, too..
            self.data_fetcher = Some(DataFetcher::new(data_provider, self.data_parallelism * 2));

            let config = match &state.model {
                Some(model) => model,
                None => {
                    warn!("Run has no model");
                    return Ok(());
                }
            };
            let model::Model::LLM(llm) = config;
            let _llm = llm.clone();
            self.available_trainers = models
                .into_iter()
                .map(|model| {
                    Trainer::new(
                        model,
                        llm.lr_schedule.into(),
                        llm.optimizer,
                        self.micro_batch_size
                            .unwrap_or(state.data_indicies_per_batch as usize),
                        self.atomic_run_state.clone(),
                    )
                })
                .collect();
            self.tokenizer = Some(Arc::new(tokenizer));

            if !self.eval_tasks.is_empty() {
                // start preparing eval tasks in background
                let eval_tasks = self.eval_tasks.drain(..).collect::<Vec<_>>();
                let eval_task_max_docs = self.eval_task_max_docs;
                let tokenizer = self.tokenizer.clone();
                self.preparing_eval_tasks = Some(tokio::task::spawn_blocking(move || {
                    match tokenizer {
                        Some(tokenizer) => Ok(eval_tasks
                            .into_iter()
                            .map(|task| task.prepare(&tokenizer, None, true, eval_task_max_docs))
                            .collect()),
                        None => {
                            bail!("No tokenizer");
                        }
                    }
                    // TODO: deal with bos tokenizers?
                }));
            }
        }

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was round state"))?
            .run_state
            == RunState::RoundTrain
        {
            return Ok(());
        }

        // transition to RoundTrain -- round start time!
        if self.applying.is_some() {
            bail!("Ready to train but previous applying still running");
        }
        if !self.evals.is_empty() {
            bail!("Ready to train but evals still running");
        }
        if self.checkpointing.is_some() {
            bail!("Ready to train but still checkpointing");
        }
        if self.available_trainers.len() != self.data_parallelism {
            bail!(
                "Missing trainers at training start, expected {} but have {}",
                self.data_parallelism,
                self.available_trainers.len()
            );
        }

        let round = state.current_round()?;

        let now = Instant::now();
        if let Some(last_round_start) = self.round_start {
            self.round_durations.push(now - last_round_start);
        }
        self.round_start = Some(Instant::now());

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            state.clients.len(),
            round.random_seed,
        );
        self.data_assignments = assign_data_for_state(state, &committee_selection);

        if self.data_fetcher.is_none() {
            bail!("Ready to train but no data fetcher! Did we miss warmup??");
        }

        let (num_batch_ids_for_this_round, training_data) = self
            .data_fetcher
            .as_mut()
            .unwrap()
            .fetch_data(state, &self.data_assignments, &self.identity);
        self.training_data = Some(training_data);

        let committee_proof = committee_selection.get_committee(index);
        let witness_proof = committee_selection.get_witness(index);
        info!(
            "Assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={}",
            state.step, round.height, state.epoch, index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness
        );
        self.blooms = match witness_proof.witness {
            true => {
                let commit_bloom = Bloom::random(
                    num_batch_ids_for_this_round * 2,
                    BLOOM_FALSE_RATE,
                    BLOOM_MAX_BITS,
                );
                let participant_bloom =
                    Bloom::random(state.clients.len(), BLOOM_FALSE_RATE, BLOOM_MAX_BITS);
                let order_bloom = Bloom::random(
                    num_batch_ids_for_this_round,
                    BLOOM_FALSE_RATE,
                    BLOOM_MAX_BITS,
                );
                debug!(
                    "Commit bloom size: {} bits, {} keys",
                    commit_bloom.bits.len(),
                    commit_bloom.keys.len()
                );
                debug!(
                    "Participant bloom size: {} bits, {} keys",
                    participant_bloom.bits.len(),
                    participant_bloom.keys.len()
                );
                debug!(
                    "Order bloom size: {} bits, {} keys",
                    order_bloom.bits.len(),
                    order_bloom.keys.len()
                );
                Some((commit_bloom, participant_bloom, order_bloom))
            }
            false => None,
        };
        self.committee_info = Some((committee_proof, witness_proof, committee_selection));
        self.prev_commitments = self.commitments.drain().collect();
        self.commitments_per_client.clear();
        self.prev_payloads = self.payloads.drain().collect();
        self.notify_train_start.notify_one();
        self._last_observed_num_batches_remaining = state.batches_per_round as usize;
        Ok(())
    }

    fn round_witness(&mut self, index: u64) -> Result<Option<Witness>> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round witness"))?;
        assert_eq!(state.run_state, RunState::RoundWitness);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was witness"))?
            .run_state
            != RunState::RoundWitness
        {
            self.start_evals();
        }

        Ok(self.get_witness_to_send(index))
    }

    async fn round_apply(&mut self) -> Result<()> {
        self.cancel_evals().await?; // CANCEL SAFETY
        self.started_early_evals = false;

        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round apply"))?;
        assert_eq!(state.run_state, RunState::RoundApply);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was apply"))?
            .run_state
            == RunState::RoundApply
        {
            return Ok(());
        }

        let trainers_still_running = self.data_parallelism - self.available_trainers.len();
        if trainers_still_running > 0 {
            bail!("Apply round but {trainers_still_running} trainer(s) aren't finished");
        } else {
            debug!(
                "Apply start ({} commitments, {} payloads)",
                self.commitments
                    .values()
                    .fold(0, |acc, ele| acc + ele.len()),
                self.payloads.len()
            );
        }

        let mut sum = 0.0;
        let count = self.round_losses.len();
        if count > 0 {
            for x in self.round_losses.drain(..) {
                sum += x;
            }
            let loss = sum / count as f32;
            info!("Step {} loss: {}", state.step, loss);
            self.losses.push(loss);
            self.wandb_log.insert("train/loss", loss);
            self.wandb_log
                .insert("train/certainty", self.certainty(loss));
        }
        if let Some(wandb_run) = &self.wandb_run {
            self.wandb_log
                .insert("train/total_tokens", self.total_tokens());
            self.wandb_log
                .insert("train/tokens_per_sec", self.global_tokens_per_second());
            self.wandb_log
                .insert("coordinator/num_clients", state.clients.len());
            self.wandb_log.insert("coordinator/epoch", state.epoch);
            self.wandb_log.insert(
                "coordinator/round",
                state
                    .current_round()
                    .ok()
                    .map(|x| x.height)
                    .unwrap_or_default(),
            );
            self.wandb_log.insert("_step", state.step);
            let wandb_log = std::mem::take(&mut self.wandb_log);
            let wandb_run = wandb_run.clone();
            tokio::spawn(async move { wandb_run.log(wandb_log).await });
        }

        self.apply_start = Some(Instant::now());

        let trainers = self.available_trainers.drain(..).collect::<Vec<_>>();
        let round_rollbacks = self.round_rollbacks.clone();
        let step = state.step;
        let witness_quorum = state.witness_quorum;

        let mut payloads: HashMap<psyche_network::Hash, PayloadState<T>> = match state.overlapped {
            true => self.prev_payloads.drain().collect(),
            false => self.payloads.drain().collect(),
        };
        let commitments: HashMap<u64, Vec<(T, TrainingResult)>> = match state.overlapped {
            true => self.prev_commitments.drain().collect(),
            false => self.commitments.drain().collect(),
        };

        if state.overlapped && state.first_round {
            // in overlapped mode the first training step of each epoch has no apply phase.
            // this is so that on the trainer we can we overlap the uploading
            // of the last step's results while concurrently computing the next
            // step. this skip "primes" the pump
            info!("First round of epoch in overlap mode, skipping apply");
            self.applying = Some(tokio::task::spawn(async move {
                round_rollbacks.lock().await.push((step, Vec::new()));
                Ok(trainers)
            }));
        } else {
            assert!(!payloads.is_empty());
            assert!(!commitments.is_empty());
            let round = match state.overlapped {
                true => state.prev_round()?,
                false => state.current_round()?,
            };
            let witnesses = round.witnesses.clone();
            let batch_ids = get_batch_ids_for_round(round, state);
            self.applying = Some(tokio::task::spawn(async move {
                let mut distro_results: Vec<Vec<DistroResult>> = Vec::new();

                for batch_id in batch_ids {
                    let batch_commitments = match commitments.get(&batch_id) {
                        Some(x) => x,
                        None => {
                            warn!("DESYNC: No commitments for batch {}", batch_id);
                            continue;
                        }
                    };
                    let consensus = match Coordinator::<T>::select_consensus_commitment_by_witnesses(
                        &batch_commitments
                            .iter()
                            .map(|x| x.1.commitment)
                            .collect::<Vec<_>>(),
                        &witnesses,
                        witness_quorum,
                    ) {
                        Some(x) => x,
                        None => {
                            warn!("DESYNC: No consensus commitment for batch {}", batch_id);
                            continue;
                        }
                    };
                    let consensus = &batch_commitments[consensus].1;
                    let maybe_results = match payloads.remove(&consensus.ticket.hash()) {
                        Some(PayloadState::Deserializing(x)) => match x.is_finished() {
                            true => x.await.unwrap(),
                            false => {
                                bail!("DESYNC: Did not finish downloading payload for consensus commitment 0x{} for batch {}", hex::encode(consensus.commitment), batch_id);
                            }
                        },
                        _ => {
                            bail!("DESYNC: Did not begin downloading payload for consensus commitment 0x{} for batch {}", hex::encode(consensus.commitment), batch_id);
                        }
                    };

                    match maybe_results {
                        Ok(results) => {
                            distro_results.push(results);
                        }
                        Err(err) => warn!("DESYNC: Got the following error when deserializing results for commitment 0x{}: {}", hex::encode(consensus.commitment), err),
                    }
                }

                round_rollbacks
                    .lock()
                    .await
                    .push((step, distro_results.clone()));

                let futures: Vec<JoinHandle<Result<Trainer>>> = trainers
                    .into_iter()
                    .map(|trainer| {
                        let distro_results = distro_results.clone();

                        tokio::task::spawn_blocking(move || {
                            trainer.apply_distro_results(step, distro_results)
                        })
                    })
                    .collect::<Vec<_>>();
                let mut trainers = Vec::new();
                for future in futures {
                    trainers.push(future.await??);
                }
                Ok(trainers)
            }));
        }

        let (_, witness_proof, committee_selection) = self
            .committee_info
            .take()
            .ok_or(Error::msg("No committee info in apply"))?;

        if witness_proof.witness {
            let witnesses = state.current_round()?.witnesses.clone();
            let witness_quorum = state.witness_quorum;
            let clients = state.clients.clone();
            self.health_checking = Some(tokio::task::spawn_blocking(move || {
                let mut checks = HealthChecks::new();
                for (index, client) in clients.into_iter().enumerate() {
                    let proof = committee_selection.get_committee(index as u64);
                    if proof.committee == Committee::Trainer
                        && !Coordinator::trainer_healthy_by_witnesses(
                            &client,
                            &witnesses,
                            witness_quorum,
                        )
                    {
                        checks.push(proof);
                    }
                }
                Ok(checks)
            }));
        }

        Ok(())
    }

    fn cooldown(&mut self) -> Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round witness"))?;
        assert_eq!(state.run_state, RunState::Cooldown);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was cooldown"))?
            .run_state
            != RunState::Cooldown
        {
            // todo consider allowing ability to write checkpoint to disk without uploading to HF
            if let Some(CheckpointUploadInfo {
                hub_repo,
                hub_token,
                checkpoint_dir,
            }) = self.checkpoint_upload_info.clone()
            {
                match self.available_trainers.pop() {
                    Some(trainer) => {
                        let step = state.step - 1;
                        let run_id = state.run_id.clone();
                        let checkpoint_dir = checkpoint_dir.clone();
                        let checkpoint_extra_files = self.checkpoint_extra_files.clone();
                        self.checkpointing = Some(tokio::task::spawn(async move {
                            let (variables, trainer) =
                                tokio::task::spawn_blocking(|| trainer.extract()).await??;

                            let path = checkpoint_dir.join(format!("{run_id}-step{step}"));

                            info!("Saving to {}", path.display());

                            let mut local = tokio::task::spawn_blocking({
                                let path = path.clone();
                                move || save_tensors_into_safetensors(variables, path)
                            })
                            .await??;

                            for extra in checkpoint_extra_files {
                                let to = path.join(extra.file_name().unwrap());
                                tokio::fs::copy(extra.clone(), to.clone()).await?;
                                local.push(to);
                            }

                            let hub_repo = {
                                info!("Uploading to {}", hub_repo);
                                let revision = upload_model_repo_async(
                                    hub_repo.clone(),
                                    local,
                                    hub_token.clone(),
                                    Some(format!("step {step}")),
                                    None,
                                )
                                .await?;
                                Some(model::HubRepo {
                                    repo_id: hub_repo.clone(),
                                    revision: Some(revision),
                                })
                            };
                            Ok((trainer, hub_repo))
                        }));
                    }
                    None => {
                        bail!("No available trainers for checkpointing");
                    }
                }
            } else {
                self.start_evals();
            }
        }

        Ok(())
    }

    async fn load_data_and_model(
        identity: T,
        private_key: T::PrivateKey,
        model: model::Model,
        data_parallelism: usize,
        tensor_parallelism: usize,
        hub_token: Option<String>,
        wandb_info: Option<WandBInfo>,
    ) -> Result<LoadedModelAndData<T>> {
        let model::Model::LLM(llm) = model;
        let data_future = match &llm.data_location {
            model::LLMTrainingDataLocation::Server(data_server) => {
                DataProviderTcpClient::connect(data_server, identity, private_key)
            }
            model::LLMTrainingDataLocation::Local(_) => todo!(),
        };
        let model_future: JoinHandle<Result<RawLoadedModel>> = match &llm.architecture {
            model::LLMArchitecture::HfLlama => match &llm.checkpoint {
                model::Checkpoint::Hub(hub_repo) => {
                    let hub_repo = hub_repo.clone();
                    tokio::spawn(async move {
                        let local = PathBuf::from(hub_repo.repo_id.clone());
                        let repo_files = match hub_repo.revision.is_none()
                            && tokio::fs::try_exists(local.clone())
                                .await
                                .unwrap_or_default()
                        {
                            true => {
                                let mut ret = Vec::new();
                                let mut read_dir = tokio::fs::read_dir(local).await?;
                                while let Some(dir_entry) = read_dir.next_entry().await? {
                                    ret.push(dir_entry.path())
                                }
                                ret
                            }
                            false => {
                                info!("Downloading {}", hub_repo.repo_id);
                                download_model_repo_async(
                                    hub_repo.repo_id.clone(),
                                    hub_repo.revision,
                                    None,
                                    hub_token,
                                    None,
                                    false,
                                )
                                .await?
                            }
                        };
                        let checkpoint_extra_files = repo_files
                            .iter()
                            .filter(|file| {
                                file.ends_with("config.json")
                                    || file.ends_with("tokenizer.json")
                                    || file.ends_with("tokenizer_config.json")
                                    || file.ends_with("special_tokens_map.json")
                                    || file.ends_with("generation_config.json")
                            })
                            .cloned()
                            .collect();
                        info!("Loading {}", hub_repo.repo_id);
                        let mut futures = Vec::with_capacity(data_parallelism * tensor_parallelism);
                        for dp in 0..data_parallelism {
                            let communicator_id = Arc::new(CommunicatorId::new());
                            for tp in 0..tensor_parallelism {
                                let tensor_parallelism_world = match tensor_parallelism {
                                    1 => None,
                                    tensor_parallelism => {
                                        Some((communicator_id.clone(), tp, tensor_parallelism))
                                    }
                                };
                                let repo_files = repo_files.clone();
                                futures.push(tokio::task::spawn_blocking(move || {
                                    LlamaForCausalLM::from_pretrained(
                                        &repo_files,
                                        Some(Kind::BFloat16),
                                        None,
                                        Some(Device::Cuda(dp * tensor_parallelism + tp)),
                                        tensor_parallelism_world,
                                        Some(llm.max_seq_len as usize),
                                    )
                                }));
                            }
                        }
                        let tokenizer = auto_tokenizer(&repo_files)?;
                        let mut models = Vec::new();
                        for future in futures {
                            models.push(future.await??);
                        }
                        info!(
                            "Loaded {} onto {} gpu(s) (dp={},tp={})",
                            hub_repo.repo_id,
                            data_parallelism * tensor_parallelism,
                            data_parallelism,
                            tensor_parallelism
                        );
                        Ok(RawLoadedModel {
                            models,
                            tokenizer,
                            checkpoint_extra_files,
                        })
                    })
                }
                model::Checkpoint::Ephemeral => {
                    bail!("Joined an ephemeral run, cannot load model")
                }
            },
        };
        let wandb_future: JoinHandle<Result<Option<wandb::Run>>> = tokio::spawn(async move {
            match wandb_info {
                Some(wandb_info) => {
                    let wandb = wandb::WandB::new(wandb::BackendOptions::new(wandb_info.api_key));
                    let mut run_info = wandb::RunInfo::new(wandb_info.project).name(wandb_info.run);
                    if let Some(entity) = wandb_info.entity {
                        run_info = run_info.entity(entity);
                    }
                    Ok(Some(wandb.new_run(run_info.build()?).await?))
                }
                None => Ok(None),
            }
        });
        let (data, models, wandb_run) = tokio::join!(data_future, model_future, wandb_future);
        let RawLoadedModel {
            models,
            tokenizer,
            checkpoint_extra_files,
        } = models??;
        let data = data?;
        let wandb_run = wandb_run??;
        let mut tp_models = Vec::new();
        for model in models {
            if tp_models
                .last()
                .map(|x: &ParallelModels| x.len() == tensor_parallelism)
                .unwrap_or(true)
            {
                tp_models.push(Vec::with_capacity(tensor_parallelism));
            }
            tp_models.last_mut().unwrap().push(model);
        }
        Ok(LoadedModelAndData {
            data_provider: data,
            models: tp_models,
            tokenizer,
            checkpoint_extra_files,
            wandb_run,
        })
    }

    fn is_run_state(&self, run_state: RunState) -> bool {
        self.state
            .as_ref()
            .is_some_and(|x| x.run_state == run_state)
    }

    fn get_witness_to_send(&mut self, index: u64) -> Option<Witness> {
        if let Some((_, witness_proof, _)) = self.committee_info.as_ref() {
            if witness_proof.witness {
                let blooms = self.blooms.take();
                if let Some((commit_bloom, participant_bloom, order_bloom)) = blooms {
                    info!("Submitting witness blooms");
                    return Some(Witness {
                        index,
                        proof: *witness_proof,
                        commit_bloom,
                        participant_bloom,
                        order_bloom,
                    });
                }
            }
        }
        None
    }

    fn start_evals(&mut self) {
        if !self.prepared_eval_tasks.is_empty() && !self.available_trainers.is_empty() {
            self.eval_cancel.store(false, Ordering::SeqCst);
            debug!(
                "Starting evals {:?} on {} trainers",
                self.prepared_eval_tasks
                    .iter()
                    .map(|x| x.task.name())
                    .collect::<Vec<_>>(),
                self.available_trainers.len()
            );
            self.evals = self
                .available_trainers
                .drain(..)
                .enumerate()
                .map(|(dp_index, mut trainer)| {
                    let data_parallelism = self.data_parallelism;
                    let eval_cancel = self.eval_cancel.clone();
                    let prepared_eval_tasks = self.prepared_eval_tasks.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut stop = false;
                        while !stop {
                            let mut iter = prepared_eval_tasks
                                .iter()
                                .zip(
                                    prepared_eval_tasks
                                        .iter()
                                        .map(|x| x.next_index.load(Ordering::SeqCst))
                                        .collect::<Vec<_>>(),
                                )
                                .collect::<Vec<_>>();
                            iter.shuffle(&mut thread_rng());
                            for (eval_task, next_index) in iter {
                                if eval_cancel.load(Ordering::SeqCst) {
                                    stop = true;
                                    break;
                                }
                                let result = eval_task.task.run(
                                    &mut trainer,
                                    true,
                                    Some((next_index + dp_index, data_parallelism)),
                                    Some(eval_task.results.clone()),
                                    Some(eval_cancel.clone()),
                                    Some(10),
                                    true,
                                );
                                eval_task
                                    .next_index
                                    .fetch_max(result.next_index, Ordering::SeqCst);
                            }
                        }
                        Ok(trainer)
                    })
                })
                .collect();
        }
    }

    // cancel safe
    async fn cancel_evals(&mut self) -> Result<()> {
        if !self.eval_cancel.swap(true, Ordering::SeqCst) {
            debug!("Cancelling evals");
        }
        while !self.evals.is_empty() {
            if let Some(finished) = self.evals.iter_mut().position(|x| x.is_finished()) {
                let trainer = self.evals.get_mut(finished).unwrap().await??;
                self.evals.swap_remove(finished);
                self.available_trainers.push(trainer);
                if self.evals.is_empty() {
                    let mut last_eval_results = HashMap::new();
                    for eval_task in &self.prepared_eval_tasks {
                        let metric_name: &str = eval_task.task.main_metric_name();
                        let task_name = eval_task.task.name();
                        match eval_task.results.sample(metric_name) {
                            Some(metric) => {
                                last_eval_results.insert(task_name.to_owned(), metric);
                                info!("{} {}: {:.3}", task_name, metric_name, metric)
                            }
                            None => {
                                warn!("{} missing metric {}", task_name, metric_name)
                            }
                        }
                    }
                    for (key, value) in last_eval_results {
                        self._eval_results
                            .entry(key.clone())
                            .or_default()
                            .push(value);

                        self.wandb_log.insert(
                            format!(
                                "eval/{}",
                                key.to_lowercase()
                                    .chars()
                                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                    .collect::<String>()
                            ),
                            value,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn maybe_write_gradients(&self, payload: &Payload) {
        if let Some(write_gradients_dir) = &self.write_gradients_dir {
            if let Payload::DistroResult(distro_result) = payload {
                info!("Trying to write distro result to disk...");
                if let Err(e) = fs::create_dir_all(write_gradients_dir) {
                    warn!("Failed to create write_gradients_dir: {e}");
                    return;
                };

                let fname = format!(
                    "result-step{}-batch{}.vec-postcard",
                    distro_result.step, distro_result.batch_id
                );
                let fpath = write_gradients_dir.join(&fname);
                let serialized = match disto_results_to_bytes(&distro_result.distro_results) {
                    Err(e) => {
                        error!("Failed to serialize distro result data {fname} to bytes {e}");
                        return;
                    }
                    Ok(bin) => bin,
                };
                tokio::task::spawn({
                    async move {
                        match tokio::fs::write(fpath, serialized).await {
                            Ok(()) => info!("Wrote distro result {fname}."),
                            Err(e) => {
                                error!(
                                    "Failed to write serialized distro result data {fname}: {e}"
                                );
                            }
                        }
                    }
                });
            }
        }
    }

    fn global_tokens_per_second(&self) -> f32 {
        match self.round_durations.is_empty() {
            true => 0.,
            false => match &self.state {
                Some(coordinator) => match &coordinator.model {
                    Some(model::Model::LLM(llm)) => match llm.data_type {
                        model::LLMTrainingDataType::Pretraining => {
                            let tokens = coordinator.batches_per_round
                                * coordinator.data_indicies_per_batch
                                * llm.max_seq_len;
                            let seconds = self
                                .round_durations
                                .iter()
                                .fold(0f32, |acc, ele| acc + ele.as_secs_f32());
                            tokens as f32 / (seconds / self.round_durations.len() as f32)
                        }
                        model::LLMTrainingDataType::Finetuning => todo!(),
                    },
                    None => 0.,
                },
                None => 0.,
            },
        }
    }

    fn total_tokens(&self) -> u64 {
        self.state
            .as_ref()
            .and_then(|x| x.current_round().ok().map(|y| y.data_index))
            .unwrap_or_default()
            * match &self.state {
                Some(coordinator) => match &coordinator.model {
                    Some(model::Model::LLM(llm)) => match llm.data_type {
                        model::LLMTrainingDataType::Pretraining => llm.max_seq_len as u64,
                        model::LLMTrainingDataType::Finetuning => todo!(),
                    },
                    None => 0,
                },
                None => 0,
            }
    }

    // normalized metric for how "certain" a model is, regardless of vocab size.
    // 1.0 indicates completely certain (no loss), 0.0 indicates random guessing, negative values are worse than guessing
    fn certainty(&self, loss: f32) -> f32 {
        match &self.tokenizer {
            Some(tokenizer) => {
                let max_entropy = (tokenizer.get_vocab_size(false) as f32).log2();
                1.0 - (loss / max_entropy)
            }
            None => 0.,
        }
    }
}

impl<T: NodeIdentity> From<&State<T>> for ClientTUIState {
    fn from(value: &State<T>) -> Self {
        let coordinator = value.state.as_ref();
        let committee = value.committee_info.as_ref().map(|x| x.0.committee);
        ClientTUIState {
            step: coordinator.map(|x| x.step).unwrap_or_default(),
            committee,
            run_state: coordinator.map(|x| x.into()).unwrap_or_default(),
            loss: value.losses.clone(),
            batches_left: value._last_observed_num_batches_remaining,
            global_tokens_per_second: value.global_tokens_per_second(),
            total_tokens: value.total_tokens(),
            evals: value._eval_results.clone(),
        }
    }
}

struct RawLoadedModel {
    pub models: Vec<LlamaForCausalLM>,
    pub tokenizer: Tokenizer,
    pub checkpoint_extra_files: Vec<PathBuf>,
}

struct LoadedModelAndData<T: NodeIdentity> {
    data_provider: DataProviderTcpClient<T>,
    models: Vec<ParallelModels>,
    tokenizer: Tokenizer,
    checkpoint_extra_files: Vec<PathBuf>,
    wandb_run: Option<wandb::Run>,
}
