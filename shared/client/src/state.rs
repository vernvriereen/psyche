use crate::{
    disto_results_to_bytes,
    fetch_data::{Batch, BatchId, DataFetcher, TrainingDataForStep},
    protocol::TrainingResult,
    trainer::{ApplyDistroResultError, ParallelModels, TrainOutput, Trainer},
    tui::ClientTUIState,
    BroadcastMessage, Payload, PeerAnnouncement, SerializedDistroResult, WandBInfo,
};
use anyhow::{anyhow, bail, Error, Result};
use psyche_coordinator::{
    assign_data_for_state, get_batch_ids_for_round, model, Committee, CommitteeProof,
    CommitteeSelection, Coordinator, HealthChecks, RunState, Witness, WitnessProof,
    BLOOM_FALSE_RATE, BLOOM_MAX_BITS,
};
use psyche_core::{sha256, Bloom, BoundedQueue, IntervalTree, NodeIdentity, RunningAverage};
use psyche_data_provider::{
    download_model_repo_async, upload_model_repo_async, DataProviderTcpClient,
};
use psyche_eval::EvalTaskOptions;
use psyche_modeling::{
    auto_tokenizer, save_tensors_into_safetensors, CommunicatorId, DistroResult, LlamaForCausalLM,
};
use psyche_network::{dummy_blob_ticket, BlobTicket, NetworkEvent, PublicKey};
use psyche_watcher::{Backend, BackendWatcher};
use rand::{seq::SliceRandom, thread_rng, Rng, RngCore};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tch::{Device, Kind};
use thiserror::Error;
use tokenizers::Tokenizer;
use tokio::{
    select,
    sync::Notify,
    task::{JoinError, JoinHandle},
    time::sleep,
};
use tracing::{debug, error, info, trace, warn};
use wandb::LogData;

const WARMUP_PEER_ANNOUNCEMENT_DURATION: Duration = Duration::from_secs(60);
const DOWNLOAD_RETRIES: usize = 3;

type TaskResult<T> = Option<JoinHandle<Result<T>>>;

#[derive(Debug)]
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

// type Rollbacks = BoundedQueue<(BatchStep, Vec<DistroResults>)>;

struct EvalTask {
    task: psyche_eval::PreparedTask,
    results: Arc<RunningAverage>,
    next_index: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub struct HubUploadInfo {
    pub hub_repo: String,
    pub hub_token: String,
}

#[derive(Debug, Clone)]
pub struct CheckpointSaveInfo {
    pub hub_upload: Option<HubUploadInfo>,
    pub checkpoint_dir: PathBuf,
}

pub enum BatchShuffleType {
    Random,
    Fixed([u8; 32]),
}

pub struct State<T: NodeIdentity> {
    pub identity: T,
    private_key: T::PrivateKey,
    data_and_model_load: TaskResult<LoadedModelAndData<T>>,
    available_trainers: Vec<Trainer>,
    trainings: Vec<JoinHandle<Result<(TrainOutput, u64)>>>,
    applying: TaskResult<Vec<Trainer>>,
    health_checking: TaskResult<HealthChecks>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    losses: Vec<f32>,
    round_losses: Vec<f32>,
    current_round: RoundState<T>,
    previous_round: RoundState<T>,
    data_parallelism: usize,
    tensor_parallelism: usize,
    batch_shuffle_type: BatchShuffleType,
    notify_train_start: Arc<Notify>,
    micro_batch_size: Option<usize>,
    write_gradients_dir: Option<PathBuf>,
    atomic_run_state: Arc<AtomicUsize>,
    //round_rollbacks: Arc<tokio::sync::Mutex<Rollbacks>>,
    data_fetcher: Option<DataFetcher<T>>,
    round_start: Option<Instant>,
    round_durations: BoundedQueue<Duration>,
    eval_cancel: Arc<AtomicBool>,
    eval_tasks: Vec<psyche_eval::Task>,
    eval_task_max_docs: Option<usize>,
    prepared_eval_tasks: Vec<Arc<EvalTask>>,
    preparing_eval_tasks: TaskResult<Vec<psyche_eval::PreparedTask>>,
    evals: Vec<JoinHandle<std::result::Result<Trainer, EvalError>>>,
    tokenizer: Option<Arc<Tokenizer>>,
    apply_start: Option<Instant>,
    training_finished_for_this_round: bool,
    checkpoint_extra_files: Vec<PathBuf>,
    checkpointing: TaskResult<(Trainer, Option<model::HubRepo>)>,
    last_warmup_peer_announcement: Option<Instant>,
    checkpoint_upload_info: Option<CheckpointSaveInfo>,
    hub_read_token: Option<String>,
    wandb_info: Option<WandBInfo>,
    wandb_run: Option<Arc<wandb::Run>>,
    wandb_log: LogData,
    retried_downloads: HashMap<psyche_network::Hash, usize>,
    optim_stats: Option<u32>,
    grad_accum_in_fp32: bool,
    /// only used for the TUI. do not rely upon this staying in sync or i will be very angy >:(
    _last_observed_num_batches_remaining: usize,
    _eval_results: HashMap<String, Vec<f64>>,
}

pub struct StateOptions<T: NodeIdentity> {
    pub identity: T,
    pub private_key: T::PrivateKey,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub eval_tasks: Vec<psyche_eval::Task>,
    pub eval_task_max_docs: Option<usize>,
    pub micro_batch_size: Option<usize>,
    pub write_gradients_dir: Option<PathBuf>,
    pub checkpoint_upload_info: Option<CheckpointSaveInfo>,
    pub hub_read_token: Option<String>,
    pub wandb_info: Option<WandBInfo>,
    pub batch_shuffle_type: BatchShuffleType,
    pub optim_stats: Option<u32>,
    pub grad_accum_in_fp32: bool,
}
struct RoundState<T: NodeIdentity> {
    height: u32,
    sent_witness: bool,
    downloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    results: HashMap<u64, Vec<(T, TrainingResult)>>,
    commitments_per_client: HashMap<T, u32>,
    data_assignments: IntervalTree<u64, T>,
    blooms: Option<(Bloom32, Bloom32, Bloom32)>,
    committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    all_batches_finished_deserializing: Arc<AtomicBool>,
    training_data: Option<TrainingDataForStep>,
}

impl<T: NodeIdentity> RoundState<T> {
    fn new() -> Self {
        Self {
            height: 0,
            sent_witness: false,
            downloads: HashMap::new(),
            results: HashMap::new(),
            commitments_per_client: HashMap::new(),
            data_assignments: IntervalTree::new(),
            blooms: None,
            committee_info: None,
            all_batches_finished_deserializing: Arc::new(AtomicBool::new(false)),
            training_data: None,
        }
    }
}

impl<T: NodeIdentity> Default for RoundState<T> {
    fn default() -> Self {
        RoundState::new()
    }
}

impl<T: NodeIdentity> RoundState<T> {
    fn get_witness_to_send(&mut self, index: u64) -> Option<Witness> {
        if self.sent_witness {
            return None;
        }
        if let Some((_, witness_proof, _)) = self.committee_info.as_ref() {
            if witness_proof.witness {
                let blooms = self.blooms.clone();
                if let Some((commit_bloom, participant_bloom, order_bloom)) = blooms {
                    info!("Submitting witness blooms");
                    self.sent_witness = true;
                    debug!("Commit bloom: {:?}", commit_bloom);
                    debug!("Participant bloom: {:?}", participant_bloom);
                    debug!("Order bloom: {:?}", order_bloom);
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
}

impl<T: NodeIdentity> State<T> {
    pub fn new(
        StateOptions {
            identity,
            private_key,
            data_parallelism,
            tensor_parallelism,
            eval_tasks,
            eval_task_max_docs,
            micro_batch_size,
            write_gradients_dir,
            checkpoint_upload_info,
            hub_read_token,
            wandb_info,
            batch_shuffle_type,
            optim_stats,
            grad_accum_in_fp32,
        }: StateOptions<T>,
    ) -> Self {
        assert!(data_parallelism > 0);
        assert!(tensor_parallelism > 0);
        assert!(micro_batch_size.map(|x| x > 0).unwrap_or(true));
        Self {
            identity,
            private_key,
            data_and_model_load: None,
            available_trainers: Vec::new(),
            trainings: Vec::new(),
            applying: None,
            health_checking: None,
            state: None,
            prev_state: None,
            current_round: RoundState::new(),
            previous_round: RoundState::new(),
            losses: Vec::new(),
            round_losses: Vec::new(),
            notify_train_start: Arc::new(Notify::new()),
            data_parallelism,
            tensor_parallelism,
            batch_shuffle_type,
            micro_batch_size,
            write_gradients_dir,
            atomic_run_state: Arc::new(AtomicUsize::new(0)),
            //round_rollbacks: tokio::sync::Mutex::new(BoundedQueue::new(NUM_STORED_ROUNDS)).into(),
            data_fetcher: None,
            round_start: None,
            round_durations: BoundedQueue::new(16),
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
            training_finished_for_this_round: false,
            last_warmup_peer_announcement: None,
            checkpoint_upload_info,
            hub_read_token,
            checkpoint_extra_files: Vec::new(),
            checkpointing: None,
            wandb_info,
            wandb_run: None,
            wandb_log: LogData::new(),
            retried_downloads: HashMap::new(),
            optim_stats,
            grad_accum_in_fp32,
            _last_observed_num_batches_remaining: 0,
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        _prev_state: Option<Coordinator<T>>,
    ) -> Result<Option<ToSend>> {
        self.state = Some(state.clone());
        let position = match state.clients.iter().position(|x| x.id == self.identity) {
            Some(position) => position as u64,
            None => {
                return Ok(None);
            }
        };
        self.atomic_run_state
            .store(state.run_state.into(), Ordering::SeqCst);
        trace!(
            "trying to tick {}. had prev state? {}",
            state.run_state,
            self.prev_state.is_some()
        );
        let tick_success_to_send: Result<Option<ToSend>, ()> = match state.run_state {
            RunState::WaitingForMembers => Ok(None),
            RunState::Warmup => Ok(self.warmup().map(|_| None)?),
            RunState::RoundTrain => match self.round_train(position).await {
                Err(TickRoundTrainError::MissedWarmup) => Err(()),
                Ok(()) => Ok(None),
                Err(other_err) => return Err(other_err.into()),
            },
            RunState::RoundWitness => match self.round_witness(position).await {
                Err(TickRoundWitnessError::MissedWarmup) => Err(()),
                Ok(witness) => Ok(witness.map(ToSend::Witness)),
                Err(other_err) => return Err(other_err.into()),
            },
            RunState::Cooldown => match self.cooldown() {
                Err(TickRoundCooldownError::MissedWarmup) => Err(()),
                Ok(()) => Ok(None),
                Err(other_err) => return Err(other_err.into()),
            },
        };
        match tick_success_to_send {
            Err(()) => {
                // we must have missed warmup. we should just wait for next WaitingForClients step.
                trace!(
                    "missed warmup, failed to transition to {}. resetting prev state to None.",
                    state.run_state
                );
                self.prev_state = None;
                Ok(None)
            }
            Ok(send) => {
                trace!("tick success for {}, setting prev state.", state.run_state);
                self.prev_state = Some(state.clone());
                Ok(send)
            }
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

        //let round_rollbacks = self.round_rollbacks.clone();
        //let handle = Handle::current();
        self.trainings.push(tokio::task::spawn_blocking(move || {
            // let rollback: Vec<_> = handle.block_on(async {
            //     round_rollbacks
            //         .lock()
            //         .await
            //         .deref()
            //         .iter()
            //         // we only want to roll back if our state is ahead,
            //         // so if we get data for e.g. step 6, but we have rollback data for steps 6, 7, 8,
            //         // this will roll back steps 6, 7, 8.
            //         .filter(|(from_round, _)| *from_round >= batch_step)
            //         .cloned()
            //         .collect()
            // });
            // if !rollback.is_empty() {
            //     debug!("Computed rollback - we are training on data for step {batch_step}, so we should roll back steps {}", rollback.iter().map(|f| f.0.to_string()).collect::<Vec<_>>().join(","));
            // }

            let output = trainer.train(batch_step, batch, vec![])?;
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
            "Training on batch {} finished, loss: {} cancelled: {}",
            batch_id, output.loss, output.cancelled
        );

        if output.cancelled || !self.is_run_state(RunState::RoundTrain) {
            return Ok(ToSend::Nothing);
        }
        if self.round_losses.is_empty() {
            for result in &output.distro_results {
                if let Some(stats) = result.stats.as_ref() {
                    for (name, value) in stats {
                        self.wandb_log.insert(format!("optim/{name}"), *value);
                    }
                }
            }
        }
        self.round_losses.push(output.loss);
        let (committee_proof, _, _) = self
            .current_round
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
        if !self.training_finished_for_this_round
            && self.available_trainers.len() == self.data_parallelism
        {
            if let Some(state) = &self.state {
                if !state.is_greedy_data() {
                    let start = if let Some(training_data) = &self.current_round.training_data {
                        // all data has been pushed, we've consumed it all, and all trainers have finished
                        training_data.assigned_ids_done.load(Ordering::SeqCst)
                            && training_data.next_sample.is_empty()
                    } else {
                        // we've already downloaded committments for all batch ids (stronger than just finished our assignments)
                        true
                    };
                    if start {
                        self.training_finished_for_this_round = true;
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
                None => Ok(ToSend::Nothing), // no repo, just local checkpoint
            },
            None => Ok(ToSend::Nothing),
        }
    }

    pub async fn poll_next(&mut self) -> Result<ToSend> {
        let trainings_finished_position = self.trainings.iter_mut().position(|x| x.is_finished());
        let is_train = self.is_run_state(RunState::RoundTrain);
        let is_warmup = self.is_run_state(RunState::Warmup);
        let opprotunistic_witness_round = match self.state.as_ref() {
            Some(state) => match state.overlapped && !state.first_round {
                true => &mut self.previous_round,
                false => &mut self.current_round,
            },
            None => &mut self.current_round,
        };
        // if is_train && !opprotunistic_witness_round.sent_witness {
        //     info!(
        //         "opprorunistic check {}: committe_info.is_some(): {}, all_batches_finished_deserializing: {}, training_finished_for_this_round: {}",
        //         opprotunistic_witness_round.height,
        //         opprotunistic_witness_round.committee_info.is_some(),
        //         opprotunistic_witness_round
        //             .all_batches_finished_deserializing
        //             .load(Ordering::SeqCst),
        //         self.training_finished_for_this_round,
        //     );
        // }
        if is_train
            && self.training_finished_for_this_round
            && opprotunistic_witness_round.committee_info.is_some()
            && (opprotunistic_witness_round
                .all_batches_finished_deserializing
                .load(Ordering::SeqCst)
                || (self
                    .state
                    .as_ref()
                    .map(|x| x.overlapped)
                    .unwrap_or_default()
                    && opprotunistic_witness_round.height <= 1))
        {
            let (_, witness_proof, _) =
                opprotunistic_witness_round.committee_info.as_ref().unwrap();
            if let Some(witness) =
                opprotunistic_witness_round.get_witness_to_send(witness_proof.index)
            {
                // send opprotunistic witness
                return Ok(ToSend::Witness(witness));
            }
        } else if is_warmup {
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
            sample = async {self.current_round.training_data.as_mut().unwrap().next_sample.recv().await}, if self.is_run_state(RunState::RoundTrain)
            && !self.available_trainers.is_empty()
            && self.current_round.training_data.is_some() => {
                match sample {
                    Some(sample) => self.handle_poll_next_training_data(sample.0, sample.1, self.current_round.training_data.as_ref().unwrap().step),
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
                        return self.handle_broadcast_from_client(public_key, message, watcher);
                    }
                    BroadcastMessage::PeerAnnouncement(announcement) => {
                        return Ok(Some(announcement.ticket.clone()))
                    }
                }
            }
            NetworkEvent::DownloadComplete(downloaded) => {
                self.retried_downloads.remove(&downloaded.hash);
                match &downloaded.data {
                    Payload::DistroResult(_) => {
                        debug!(
                            "Payload 0x{} received from {}",
                            hex::encode(downloaded.hash),
                            downloaded.from
                        );
                        self.handle_payload(downloaded.hash, downloaded.data)
                            .await?;
                    }
                    Payload::Empty { random: _ } => {}
                }
            }
            NetworkEvent::DownloadFailed(result) => {
                let retries = *self.retried_downloads.get(&result.hash).unwrap_or(&0);
                if retries >= DOWNLOAD_RETRIES {
                    warn!("Download failed (not retrying): {}", result.error);
                } else {
                    match self.current_round.downloads.get(&result.hash) {
                        Some(PayloadState::Downloading((_, _, ticket))) => {
                            info!("Download failed (retrying): {}", result.error);
                            self.retried_downloads.insert(result.hash, retries + 1);
                            return Ok(Some(ticket.clone()));
                        }
                        _ => match self.previous_round.downloads.get(&result.hash) {
                            Some(PayloadState::Downloading((_, _, ticket))) => {
                                info!("Download failed (retrying): {}", result.error);
                                self.retried_downloads.insert(result.hash, retries + 1);
                                return Ok(Some(ticket.clone()));
                            }
                            _ => {
                                info!("Missing payload for failed download 0x{}", result.hash);
                            }
                        },
                    }
                }
            }
        }
        Ok(None)
    }

    fn handle_broadcast_from_client<B: Backend<T> + 'static>(
        &mut self,
        public_key: PublicKey,
        broadcast: BroadcastMessage,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<Option<BlobTicket>> {
        match watcher.get_client_for_p2p_public_key(public_key.as_bytes()) {
            Some(client) => {
                self.handle_broadcast_from_identity(&client.id, Some(client), broadcast)
            }

            None => {
                debug!("Got broadcast from unknown client {}", public_key);
                Ok(None)
            }
        }
    }

    pub(crate) fn handle_broadcast_from_identity(
        &mut self,
        identity: &T,
        check_committee: Option<&psyche_coordinator::Client<T>>,
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
                let round_state = if state.overlapped {
                    if training_result.step == state.step {
                        debug!(
                            "Queueing download for current step {}",
                            training_result.step
                        );
                        &mut self.current_round
                    } else if training_result.step == state.step - 1 {
                        debug!(
                            "Queueing download for previous step {}",
                            training_result.step
                        );
                        &mut self.previous_round
                    } else {
                        debug!(
                            "Ignoring result from step {} (current step is {})",
                            training_result.step, state.step
                        );
                        return Ok(None);
                    }
                } else if training_result.step != state.step {
                    debug!(
                        "Ignoring result from step {} (current step is {})",
                        training_result.step, state.step
                    );
                    return Ok(None);
                } else {
                    debug!(
                        "Queueing download for current step {}",
                        training_result.step
                    );
                    &mut self.current_round
                };

                if let Some(client) = check_committee {
                    match &round_state.committee_info {
                        Some((_, _, committee_info)) => {
                            if !committee_info.verify_committee_for_client(
                                client,
                                &training_result.proof,
                                &state.clients,
                            ) {
                                debug!("Committee verification failed for commitment 0x{} (step={},batch_id={}) received from {}", hex::encode(training_result.commitment),                              training_result.step,
                                training_result.batch_id,
                                identity);
                                return Ok(None);
                            }
                        }
                        None => {
                            return Ok(None);
                        }
                    };
                }

                if training_result.proof.committee == Committee::Trainer {
                    let ticket = training_result.ticket.clone();
                    let hash = ticket.hash();

                    if round_state.downloads.contains_key(&hash) {
                        return Ok(None);
                    }

                    let client_commitments = *round_state
                        .commitments_per_client
                        .get(identity)
                        .unwrap_or(&0);
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
                        let correct_assignee = match round_state.data_assignments.get(first_data_id)
                        {
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
                    round_state
                        .commitments_per_client
                        .insert(identity.clone(), client_commitments + 1);

                    let total_commitments = round_state
                        .commitments_per_client
                        .values()
                        .fold(0, |acc, ele| acc + *ele);
                    debug!(
                        "Total commitments for step {}: {}",
                        state.step, total_commitments
                    );

                    if let Some((_, witness_proof, _)) = round_state.committee_info.as_ref() {
                        if witness_proof.witness {
                            if let Some((commit_bloom, participant_bloom, order_bloom)) =
                                &mut round_state.blooms
                            {
                                commit_bloom.add(&sha256(&training_result.commitment));
                                participant_bloom.add(&sha256(identity.as_ref()));
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
                    round_state
                        .results
                        .get_mut(&training_result.batch_id)
                        .unwrap()
                        .push((identity.clone(), training_result));
                    let download_state =
                        PayloadState::Downloading((identity.clone(), batch_id, ticket.clone()));
                    round_state.downloads.insert(hash, download_state);

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
                debug!("Got peer announcement from {}", identity);
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
                let round_state = if self.current_round.downloads.contains_key(&hash) {
                    &mut self.current_round
                } else if self.previous_round.downloads.contains_key(&hash) {
                    &mut self.previous_round
                } else {
                    debug!("Unknown download {}", hash);
                    return Ok(());
                };

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
                let (_, witness_proof, _) = round_state
                    .committee_info
                    .as_ref()
                    .ok_or(Error::msg("Payload message processor has no self proofs"))?;
                // TODO: verify payload matches commitment
                // TODO: verify shape of distro_results

                // we only care to add this to consensus & track it in batch IDs if we have any batch IDs that haven't yet been voted for.
                let (just_consumed_last_batch_id, num_left) = if let Some(TrainingDataForStep {
                    batch_ids_not_yet_trained_on,
                    ..
                }) = &mut round_state.training_data
                {
                    // TODO: how do we do witnessing for verifiers that might be training on data that's not in the normal remaining batch IDs?
                    // TODO: also we want ALL those from everyone, right?
                    let mut remaining_batch_ids = batch_ids_not_yet_trained_on.lock().await; // CANCEL SAFETY
                    if witness_proof.witness {
                        match round_state.blooms.as_mut() {
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
                    debug!(
                        "Remaining batches to download for step {}: {}",
                        distro_result.step,
                        remaining_batch_ids.len()
                    );
                    (remaining_batch_ids.is_empty(), remaining_batch_ids.len())
                } else {
                    // it was already empty, so we didn't just consume the last value.
                    debug!("Got download of {} but training data is empty", hash);
                    (false, 0)
                };
                self._last_observed_num_batches_remaining = num_left;

                // we unconditionally store every seen payload, since we're not yet sure what consensus will be on whether it's included.
                let all_batches_finished_deserializing =
                    round_state.all_batches_finished_deserializing.clone();
                let deserializing = tokio::task::spawn_blocking(move || {
                    let maybe_results: Result<Vec<DistroResult>, _> = distro_result
                        .distro_results
                        .iter()
                        .map(|x| x.try_into())
                        .collect();
                    match maybe_results {
                        Ok(results) => {
                            if just_consumed_last_batch_id {
                                debug!("Finished deserializing last batch");
                                all_batches_finished_deserializing.store(true, Ordering::SeqCst);
                            }
                            Ok(results)
                        }
                        Err(err) => bail!("Error deserializing: {}", err),
                    }
                });
                round_state
                    .downloads
                    .insert(hash, PayloadState::Deserializing(deserializing));
            }
            Payload::Empty { random: _ } => {}
        }
        Ok(())
    }

    fn warmup(&mut self) -> std::result::Result<(), EnterWarmupError> {
        let state = self.state.as_ref().ok_or(EnterWarmupError::NoState)?;
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
                                    self.wandb_info
                                        .as_ref()
                                        .map(|info| (state.clone(), info.clone())),
                                )))
                        } else {
                            return Err(EnterWarmupError::ApplyStillRunning);
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

    async fn round_train(&mut self, index: u64) -> std::result::Result<(), TickRoundTrainError> {
        self.cancel_evals().await?; // CANCEL SAFETY

        let state = self.state.as_ref().ok_or(TickRoundTrainError::NoState)?;
        assert_eq!(state.run_state, RunState::RoundTrain);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(TickRoundTrainError::MissedWarmup)?
            .run_state
            == RunState::RoundTrain
        {
            return Ok(());
        }

        // if all our states are empty (first execution), wait for the data provider and model load to finish
        if self.available_trainers.is_empty()
            && self.data_fetcher.is_none()
            && self.tokenizer.is_none()
        {
            let data_and_model_load = self
                .data_and_model_load
                .take()
                .ok_or(TickRoundTrainError::DataModelLoadNotStarted)?;
            if !data_and_model_load.is_finished() {
                return Err(TickRoundTrainError::DataModelLoadUnfinished);
            }
            let LoadedModelAndData {
                data_provider,
                models,
                tokenizer,
                checkpoint_extra_files,
                wandb_run,
            } = data_and_model_load
                .await
                .map_err(TickRoundTrainError::DataModelLoadFailedToJoin)?
                .map_err(TickRoundTrainError::DataModelLoadFailed)?; // not a cancel safety point, this should return immediately
            self.checkpoint_extra_files = checkpoint_extra_files;
            self.wandb_run = wandb_run.map(Arc::new);

            // TODO add data fetching for verifying, too..
            self.data_fetcher = Some(DataFetcher::new(
                data_provider,
                self.data_parallelism * 2,
                match self.batch_shuffle_type {
                    BatchShuffleType::Random => Box::new(|| {
                        let mut arr = [0; 32];
                        rand::thread_rng().fill(&mut arr);
                        arr
                    }),
                    BatchShuffleType::Fixed(data) => Box::new(move || data),
                },
            ));

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
                        self.optim_stats,
                        self.grad_accum_in_fp32,
                        Some(state.step),
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
                            .map(|task| task.prepare(&tokenizer, None, eval_task_max_docs))
                            .collect()),
                        None => {
                            bail!("No tokenizer");
                        }
                    }
                    // TODO: deal with bos tokenizers?
                }));
            }
        }

        // transition to RoundTrain -- round start time!
        if !self.evals.is_empty() {
            return Err(TickRoundTrainError::EvalsStillRunning);
        }
        if self.checkpointing.is_some() {
            return Err(TickRoundTrainError::CheckpointingStillRunning);
        }

        let round = state
            .current_round()
            .ok_or(TickRoundTrainError::NoActiveRound)?;

        self.apply_start = Some(Instant::now());
        let trainers = self.available_trainers.drain(..).collect::<Vec<_>>();
        if state.first_round || (state.overlapped && round.height == 1) {
            // in overlapped mode the first training step of each epoch has no apply phase.
            // this is so that on the trainer we can we overlap the uploading
            // of the last step's results while concurrently computing the next
            // step. this skip "primes" the pump
            info!("Skipping early apply");
            self.applying = Some(tokio::task::spawn(async move {
                //round_rollbacks.lock().await.push((step, Vec::new()));
                Ok(trainers)
            }));
        } else {
            //let round_rollbacks = self.round_rollbacks.clone();
            let step = state.step;
            let witness_quorum = state.witness_quorum;

            let mut payloads: HashMap<psyche_network::Hash, PayloadState<T>> =
                match state.overlapped {
                    true => std::mem::take(&mut self.previous_round.downloads),
                    false => std::mem::take(&mut self.current_round.downloads),
                };
            let commitments: HashMap<u64, Vec<(T, TrainingResult)>> = match state.overlapped {
                true => std::mem::take(&mut self.previous_round.results),
                false => std::mem::take(&mut self.current_round.results),
            };
            assert!(!payloads.is_empty());
            assert!(!commitments.is_empty());

            let witnesses = round.witnesses.clone();
            let batch_ids = get_batch_ids_for_round(
                // coordinator has already advanced to the next round but we haven't started ours yet.
                // our current_round corresponds to the coordinator's previous_round
                match state.overlapped {
                    true => state.previous_previous_round(),
                    false => state.previous_round(),
                }
                .ok_or(TickRoundTrainError::NoActiveRound)?,
                state,
            );
            self.applying = Some(tokio::task::spawn(async move {
                let mut distro_results: Vec<Vec<DistroResult>> = Vec::new();

                for batch_id in batch_ids {
                    let batch_commitments = match commitments.get(&batch_id) {
                        Some(x) => x,
                        None => {
                            warn!("No commitments for batch {}", batch_id);
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
                            warn!("No consensus commitment for batch {}", batch_id);
                            continue;
                        }
                    };
                    let consensus = &batch_commitments[consensus].1;
                    let maybe_results = match payloads.remove(&consensus.ticket.hash()) {
                        Some(PayloadState::Deserializing(x)) => match x.is_finished() {
                            true => x.await.unwrap(),
                            false => {
                                bail!("DESYNC: Did not finish deserializing payload for consensus commitment 0x{} for batch {}", hex::encode(consensus.commitment), batch_id);
                            }
                        },
                        Some(PayloadState::Downloading(_)) => {
                            bail!("DESYNC: Did not begin downloading payload for consensus commitment 0x{} for batch {}", hex::encode(consensus.commitment), batch_id);
                        }
                        None => bail!(
                            "DESYNC: Unknown consensus commitment 0x{} for batch {}",
                            hex::encode(consensus.commitment),
                            batch_id
                        ),
                    };

                    match maybe_results {
                        Ok(results) => {
                            distro_results.push(results);
                        }
                        Err(err) => warn!("DESYNC: Got the following error when deserializing results for commitment 0x{}: {}", hex::encode(consensus.commitment), err),
                    }
                }

                // round_rollbacks
                //     .lock()
                //     .await
                //     .push((step, distro_results.clone()));

                let futures: Vec<JoinHandle<std::result::Result<Trainer, ApplyDistroResultError>>> =
                    trainers
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

        if !state.first_round {
            // as with applying, the coordinator has already advanced to the next round but we haven't started ours yet.
            // our current_round corresponds to the coordinator's previous_round
            let (_, witness_proof, committee_selection) = self
                .current_round
                .committee_info
                .clone()
                .ok_or(TickRoundTrainError::NoCommitteeInfo)?;

            if witness_proof.witness {
                let witnesses = state
                    .previous_round()
                    .ok_or(TickRoundTrainError::NoActiveRound)?
                    .witnesses
                    .clone();
                let witness_quorum = state.witness_quorum;
                let clients = state.clients.clone();
                self.health_checking = Some(tokio::task::spawn_blocking(move || {
                    let mut checks = HealthChecks::new();
                    for (index, client) in clients.into_iter().enumerate() {
                        let proof = committee_selection.get_committee(index as u64);
                        if proof.committee == Committee::Trainer {
                            debug!(
                                "Trainer {:?} health score: {}",
                                client,
                                Coordinator::trainer_healthy_score_by_witnesses(
                                    &client, &witnesses
                                )
                            );
                            if !Coordinator::trainer_healthy_by_witnesses(
                                &client,
                                &witnesses,
                                witness_quorum,
                            ) {
                                debug!("Found unhealthy trainer at index {index}");
                                checks.push(proof);
                            }
                        }
                    }
                    Ok(checks)
                }));
            }
        }

        debug!("Transitioning to train step {}", state.step);

        let now = Instant::now();
        if let Some(last_round_start) = self.round_start {
            self.round_durations.push(now - last_round_start);
        }
        self.round_start = Some(Instant::now());

        self.previous_round = std::mem::take(&mut self.current_round);
        self.current_round.height = round.height;
        if self.previous_round.height == 0 && state.overlapped {
            self.previous_round.sent_witness = false; // we need to resend the witness from the first step again on real step
        }

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            state.clients.len(),
            round.random_seed,
        );
        self.current_round.data_assignments = assign_data_for_state(state, &committee_selection);
        self.training_finished_for_this_round = false;

        if self.data_fetcher.is_none() {
            return Err(TickRoundTrainError::NoDataFetcher);
        }

        let (num_batch_ids_for_this_round, training_data) = self
            .data_fetcher
            .as_mut()
            .unwrap()
            .fetch_data(state, &self.current_round.data_assignments, &self.identity);
        self.current_round.training_data = Some(training_data);

        let committee_proof = committee_selection.get_committee(index);
        let witness_proof = committee_selection.get_witness(index);
        info!(
            "Assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={}",
            state.step, round.height, state.epoch, index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness
        );
        self.current_round.blooms = match witness_proof.witness {
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
        self.current_round.committee_info =
            Some((committee_proof, witness_proof, committee_selection));
        self.notify_train_start.notify_one();
        self._last_observed_num_batches_remaining = state.batches_per_round as usize;
        Ok(())
    }

    async fn round_witness(
        &mut self,
        index: u64,
    ) -> std::result::Result<Option<Witness>, TickRoundWitnessError> {
        self.cancel_evals().await?; // CANCEL SAFETY

        let state = self.state.as_ref().ok_or(TickRoundWitnessError::NoState)?;
        assert_eq!(state.run_state, RunState::RoundWitness);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(TickRoundWitnessError::MissedWarmup)?
            .run_state
            == RunState::RoundWitness
        {
            return Ok(None);
        }

        let trainers_still_running = self.data_parallelism - self.available_trainers.len();
        if trainers_still_running > 0 {
            return Err(TickRoundWitnessError::TrainersStillRunning(
                trainers_still_running,
            ));
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
                .insert("train/perplexity", Self::perplexity(loss));
            self.wandb_log
                .insert("train/confidence", self.confidence(loss));
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
                state.current_round().map(|x| x.height).unwrap_or_default(),
            );
            self.wandb_log.insert("_step", state.step);
            let wandb_log = std::mem::take(&mut self.wandb_log);
            let wandb_run = wandb_run.clone();
            tokio::spawn(async move { wandb_run.log(wandb_log).await });
        }

        Ok(
            match self
                .state
                .as_ref()
                .map(|x| x.overlapped)
                .unwrap_or_default()
            {
                true => self.previous_round.get_witness_to_send(index),
                false => self.current_round.get_witness_to_send(index),
            },
        )
    }

    fn cooldown(&mut self) -> std::result::Result<(), TickRoundCooldownError> {
        let state = self.state.as_ref().ok_or(TickRoundCooldownError::NoState)?;
        assert_eq!(state.run_state, RunState::Cooldown);

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(TickRoundCooldownError::MissedWarmup)?
            .run_state
            != RunState::Cooldown
        {
            // todo consider allowing ability to write checkpoint to disk without uploading to HF
            if let Some(CheckpointSaveInfo {
                hub_upload,
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
                            info!("Extracting full model for save");
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

                            if let Some(HubUploadInfo {
                                hub_repo,
                                hub_token,
                            }) = hub_upload
                            {
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
                            } else {
                                Ok((trainer, None))
                            }
                        }));
                    }
                    None => {
                        return Err(TickRoundCooldownError::NoAvailableTrainersForCheckpointing)
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
        wandb_info: Option<(Coordinator<T>, WandBInfo)>,
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
                            let communicator_id = match tensor_parallelism {
                                1 => None,
                                _ => Some(Arc::new(CommunicatorId::new())),
                            };
                            for tp in 0..tensor_parallelism {
                                let tensor_parallelism_world =
                                    communicator_id.as_ref().map(|communicator_id| {
                                        (communicator_id.clone(), tp, tensor_parallelism)
                                    });
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
                Some((warmup_state, wandb_info)) => {
                    let wandb = wandb::WandB::new(wandb::BackendOptions::new(wandb_info.api_key));
                    let mut run_info = wandb::RunInfo::new(wandb_info.project)
                        .name(wandb_info.run)
                        .config((
                            (
                                "data_indicies_per_batch",
                                warmup_state.data_indicies_per_batch,
                            ),
                            ("batches_per_round", warmup_state.batches_per_round),
                            ("total_steps", warmup_state.total_steps),
                            ("rounds_per_epoch", warmup_state.rounds_per_epoch),
                            ("run_id", warmup_state.run_id),
                        ));
                    if let Some(entity) = wandb_info.entity {
                        run_info = run_info.entity(entity);
                    }
                    if let Some(group) = wandb_info.group {
                        run_info = run_info.group(group);
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
        } = models?.map_err(|err| anyhow!("model load error: {err}"))?;
        let data = data.map_err(|err| anyhow!("data load error: {err}"))?;
        let wandb_run = wandb_run?.map_err(|err| anyhow!("wandb load error: {err}"))?;
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
                                    EvalTaskOptions {
                                        model: &mut trainer,
                                        skip_and_step_by: Some((
                                            next_index + dp_index,
                                            data_parallelism,
                                        )),
                                        live_results: Some(eval_task.results.clone()),
                                        cancel: Some(eval_cancel.clone()),
                                        limit: Some(10),
                                        loop_if_empty: true,
                                    },
                                    false,
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
    async fn cancel_evals(&mut self) -> std::result::Result<(), FinishEvalsError> {
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
                    "result-{}-step{}-batch{}.vec-postcard",
                    self.identity, distro_result.step, distro_result.batch_id
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
            .and_then(|x| x.current_round().map(|y| y.data_index))
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

    // normalized metric for how "confident" a model is, regardless of vocab size.
    // 1.0 indicates completely certain (no loss), 0.0 indicates random guessing, negative values are worse than guessing
    fn confidence(&self, loss: f32) -> f32 {
        match &self.tokenizer {
            Some(tokenizer) => {
                let max_entropy = (tokenizer.get_vocab_size(false) as f32).log2();
                1.0 - (loss / max_entropy)
            }
            None => 0.,
        }
    }

    fn perplexity(loss: f32) -> f32 {
        loss.exp()
    }
}

impl<T: NodeIdentity> From<&State<T>> for ClientTUIState {
    fn from(value: &State<T>) -> Self {
        let coordinator = value.state.as_ref();
        let committee = value
            .current_round
            .committee_info
            .as_ref()
            .map(|x| x.0.committee);
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

#[derive(Error, Debug)]
enum TickRoundWitnessError {
    #[error("round entered with no state")]
    NoState,

    #[error("this was the first state seen, we must be mid-epoch.")]
    MissedWarmup,

    #[error("couldn't cancel evals")]
    EvalCancelFailed(#[from] FinishEvalsError),

    #[error("{0} trainer(s) aren't finished")]
    TrainersStillRunning(usize),
}

#[derive(Error, Debug)]
enum TickRoundCooldownError {
    #[error("this was the first state seen, we must be mid-epoch.")]
    MissedWarmup,

    #[error("no trainers available for checkpointing")]
    NoAvailableTrainersForCheckpointing,

    #[error("round entered with no state")]
    NoState,
}

#[derive(Error, Debug)]
enum TickRoundTrainError {
    #[error("no round active")]
    NoActiveRound,

    #[error("this was the first state seen, we must be mid-epoch.")]
    MissedWarmup,

    #[error("couldn't cancel evals")]
    EvalCancelFailed(#[from] FinishEvalsError),

    #[error("evals still running")]
    EvalsStillRunning,

    #[error("checkpointing still running")]
    CheckpointingStillRunning,

    #[error("round entered with no state")]
    NoState,

    #[error("ready to train but no data fetcher! Did we miss warmup??")]
    NoDataFetcher,

    #[error("Data and model load not finished when round started!")]
    DataModelLoadUnfinished,

    #[error("Data and model load not started! Did we miss warmup??")]
    DataModelLoadNotStarted,

    #[error("failed to load data and model: {0}")]
    DataModelLoadFailed(#[from] anyhow::Error),

    #[error("failed to join data and model load task {0}")]
    DataModelLoadFailedToJoin(#[from] JoinError),

    #[error("No committee info")]
    NoCommitteeInfo,
}

#[derive(Error, Debug)]
enum EnterWarmupError {
    #[error("round entered with no state")]
    NoState,

    #[error("apply still running")]
    ApplyStillRunning,
}

#[derive(Error, Debug)]
enum EvalError {}

#[derive(Error, Debug)]
enum FinishEvalsError {
    #[error("failed to join task {0}")]
    JoinTask(#[from] JoinError),

    #[error("eval failed {0}")]
    EvalFailed(#[from] EvalError),
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
