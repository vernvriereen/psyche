use crate::{
    disto_results_to_bytes,
    fetch_data::{fetch_data, Batch, BatchId},
    trainer::{ParallelModels, TrainOutput, Trainer},
    tui::ClientTUIState,
    BroadcastMessage, Payload, SerializedDistroResult,
};
use anyhow::{bail, Error, Result};
use hex;
use psyche_coordinator::{
    get_batch_ids_for_state, model, Committee, CommitteeProof, CommitteeSelection, Coordinator,
    HealthChecks, RunState, Witness, WitnessProof, BLOOM_FALSE_RATE, BLOOM_MAX_BITS,
};
use psyche_core::{sha256, Bloom, NodeIdentity};
use psyche_data_provider::{download_model_repo_async, DataProviderTcpClient};
use psyche_modeling::{CommunicatorId, DistroResult, LlamaForCausalLM};
use psyche_network::{dummy_blob_ticket, BlobTicket, NetworkEvent};
use psyche_watcher::{Backend, BackendWatcher};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tch::{Device, Kind};
use tokio::{
    sync::{mpsc, Mutex, Notify},
    task::JoinHandle,
    time::sleep,
};
use tracing::{debug, error, info, warn};

type TaskResult<T> = Option<JoinHandle<Result<T>>>;

enum PayloadState<T: NodeIdentity> {
    Downloading((T, u64)),
    #[allow(dead_code)]
    Downloaded(Payload),
}

pub type BroadcastAndPayload = (BroadcastMessage, Payload);

pub enum ToSend {
    Nothing,
    Broadcast(BroadcastAndPayload),
    Witness(Witness),
    HealthCheck(HealthChecks),
}

pub struct State<T: NodeIdentity> {
    pub identity: T,
    private_key: T::PrivateKey,
    showed_inclusion_message: bool,
    data_and_model_load: TaskResult<(DataProviderTcpClient<T>, Vec<ParallelModels>)>,
    data_receiver: Option<mpsc::Receiver<(BatchId, Batch)>>,
    trainers: Vec<Trainer>,
    trainings: Vec<JoinHandle<Result<(TrainOutput, u64)>>>,
    applying: TaskResult<Vec<Trainer>>,
    health_checking: TaskResult<HealthChecks>,
    committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    commitments: HashMap<u64, Vec<(T, BroadcastMessage)>>,
    commitments_per_client: HashMap<T, u32>,
    payloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    blooms: Option<(Bloom<[u8; 32]>, Bloom<[u8; 32]>, Bloom<[u8; 32]>)>,
    losses: Vec<f32>,
    round_losses: Vec<f32>,
    remaining_batch_ids: Arc<Mutex<HashSet<u64>>>,
    num_remaining_batch_ids: usize,
    data_parallelism: usize,
    tensor_parallelism: usize,
    notify_poll_next: Arc<Notify>,
    notify_clear_uploads: Arc<Notify>,
    notify_new_batch: Arc<Notify>,
    micro_batch_size: Option<usize>,
    write_gradients_dir: Option<PathBuf>,
    atomic_run_state: Arc<AtomicUsize>,
}

impl<T: NodeIdentity> State<T> {
    pub fn new(
        identity: T,
        private_key: T::PrivateKey,
        data_parallelism: usize,
        tensor_parallelism: usize,
        micro_batch_size: Option<usize>,
        write_gradients_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            identity,
            private_key,
            showed_inclusion_message: false,
            data_and_model_load: None,
            data_receiver: None,
            trainers: Vec::new(),
            trainings: Vec::new(),
            applying: None,
            health_checking: None,
            committee_info: None,
            state: None,
            prev_state: None,
            blooms: None,
            commitments: HashMap::new(),
            commitments_per_client: HashMap::new(),
            payloads: HashMap::new(),
            notify_poll_next: Arc::new(Notify::new()),
            losses: Vec::new(),
            round_losses: Vec::new(),
            remaining_batch_ids: Arc::new(Mutex::new(HashSet::new())),
            num_remaining_batch_ids: 0,
            notify_clear_uploads: Arc::new(Notify::new()),
            data_parallelism,
            tensor_parallelism,
            notify_new_batch: Arc::new(Notify::new()),
            micro_batch_size,
            write_gradients_dir,
            atomic_run_state: Arc::new(AtomicUsize::new(0)),
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
            RunState::RoundWitness => {
                return self
                    .round_witness(position)
                    .map(|x| x.map(|y| ToSend::Witness(y)))
            }
            RunState::RoundApply => return self.round_apply().await.map(|_| None),
        }
    }

    pub fn get_clear_downloads_notification(&self) -> Arc<Notify> {
        self.notify_clear_uploads.clone()
    }

    pub async fn poll_next(&mut self) -> Result<ToSend> {
        if let Some(applying) = &mut self.applying {
            let trainers = applying.await??;
            self.applying = None;
            self.trainers = trainers;
        } else if self.is_run_state(RunState::RoundTrain)
            && !self.trainers.is_empty()
            && self.data_receiver.is_some()
            && self.num_remaining_batch_ids != 0
        {
            let (batch_id, batch) = self
                .data_receiver
                .as_mut()
                .unwrap()
                .recv()
                .await
                .ok_or(Error::msg("Data fetcher exited"))?;

            let state = self
                .state
                .as_ref()
                .ok_or(Error::msg("Data fetch finished, but no state"))?;

            let trainer: Trainer = self
                .trainers
                .pop()
                .ok_or(Error::msg("Round start but no trainer object (didn't finish training previous round or applying it?)"))?;
            let step: usize = state.step as usize;
            let notify = self.notify_poll_next.clone();
            self.trainings.push(tokio::task::spawn_blocking(move || {
                let output = trainer.train(step as usize, batch)?;
                notify.notify_one(); // wake up poll_next to process this
                Ok((output, batch_id))
            }));
        } else if let Some(finished) = self.trainings.iter_mut().position(|x| x.is_finished()) {
            let (output, batch_id) = self.trainings.get_mut(finished).unwrap().await??;
            self.trainings.swap_remove(finished);
            self.trainers.push(output.trainer);
            if output.cancelled || !self.is_run_state(RunState::RoundTrain) {
                return Ok(ToSend::Nothing);
            }
            self.round_losses.push(output.loss);
            debug!("Batch {} loss: {}", batch_id, output.loss);
            let (committee_proof, _, _) = self
                .committee_info
                .as_ref()
                .ok_or(Error::msg("Training complete but no self proofs"))?;
            let mut committment = Vec::with_capacity(40);
            committment.extend_from_slice(self.identity.as_ref());
            committment.extend_from_slice(&batch_id.to_be_bytes());
            let commitment = sha256(&committment);
            let step = output.step as u64;
            let broadcast = BroadcastMessage {
                step,
                batch_id,
                commitment,
                ticket: dummy_blob_ticket(),
                proof: *committee_proof,
            };
            let payload = Payload {
                step,
                batch_id,
                distro_results: output
                    .distro_results
                    .iter()
                    .map(SerializedDistroResult::try_from)
                    .collect::<std::result::Result<Vec<_>, tch::TchError>>()?,
            };

            self.maybe_write_gradients(&payload);

            return Ok(ToSend::Broadcast((broadcast, payload)));
        } else if let Some(health_checking) = &mut self.health_checking {
            let health_checks = health_checking.await??;
            self.health_checking = None;
            if !health_checks.is_empty() {
                info!(
                    "Sending health check for following indicies: {:?}",
                    health_checks
                );
                return Ok(ToSend::HealthCheck(health_checks));
            }
        } else if self.is_run_state(RunState::RoundTrain)
            && self.committee_info.is_some()
            && self.num_remaining_batch_ids == 0
        {
            let (_, witness_proof, _) = self.committee_info.as_ref().unwrap();
            if let Some(witness) = self.get_witness_to_send(witness_proof.index) {
                // send opprotunistic witness
                return Ok(ToSend::Witness(witness));
            } else {
                self.notify_poll_next.notified().await;
            }
        } else {
            self.notify_poll_next.notified().await;
        }
        Ok(ToSend::Nothing)
    }

    pub async fn process_network_event<B: Backend<T> + 'static>(
        &mut self,
        event: NetworkEvent<BroadcastMessage, Payload>,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<Option<BlobTicket>> {
        debug!("Got network event {event:?}");
        match event {
            NetworkEvent::MessageReceived((public_key, message)) => {
                // verify they are who they say they are
                debug!(
                    "Commitment 0x{} (step={},batch_id={}) received from {}",
                    hex::encode(message.commitment),
                    message.step,
                    message.batch_id,
                    public_key
                );
                if let Some(state) = &self.state {
                    if state.step == message.step as u32 {
                        if let Some((_, _, committee_selection)) = self.committee_info.as_ref() {
                            if let Some(client) =
                                watcher.get_client_for_p2p_public_key(public_key.as_bytes())
                            {
                                if committee_selection.verify_committee_for_client(
                                    client,
                                    &message.proof,
                                    &state.clients,
                                ) {
                                    return self.handle_broadcast(&client.id, message);
                                }
                            }
                        }
                    } else {
                        info!(
                            "Got broadcast for step {} from {} but current step is {}",
                            message.step, public_key, state.step
                        );
                    }
                }
            }
            NetworkEvent::DownloadComplete(downloaded) => {
                debug!(
                    "Payload 0x{} received from {}",
                    hex::encode(downloaded.hash),
                    downloaded.from
                );
                if let Some(state) = &self.state {
                    if state.step == downloaded.data.step as u32 {
                        self.handle_payload(downloaded.hash, downloaded.data)
                            .await?;
                    } else {
                        info!(
                            "Got payload for step {} from {} but current step is {}",
                            downloaded.data.step, downloaded.from, state.step
                        );
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
        let (_, witness_proof, _) = self
            .committee_info
            .as_ref()
            .ok_or(Error::msg("Broadcast message processor has no self proofs"))?;
        // verified by process_network_event caller
        if broadcast.proof.committee == Committee::Trainer {
            let client_commitments = *self.commitments_per_client.get(identity).unwrap_or(&0);
            if client_commitments
                >= self
                    .state
                    .as_ref()
                    .map(|x| x.max_batches_per_client)
                    .unwrap_or(0)
            {
                info!(
                    "Maximum commitments received from {}, dropping {}",
                    identity,
                    hex::encode(broadcast.commitment)
                );
                return Ok(None);
            }
            self.commitments_per_client
                .insert(identity.clone(), client_commitments + 1);

            if witness_proof.witness {
                match self.blooms.as_mut() {
                    Some((commit_bloom, _, _)) => commit_bloom.add(&sha256(&broadcast.commitment)),
                    None => {
                        debug!(
                            "Already submitted witness, not adding commitment 0x{} to commit bloom",
                            hex::encode(broadcast.commitment)
                        );
                    }
                }
            }
            if !self.commitments.contains_key(&broadcast.batch_id) {
                self.commitments.insert(broadcast.batch_id, Vec::new());
            }
            let ticket = broadcast.ticket.clone();
            let batch_id = broadcast.batch_id;
            self.commitments
                .get_mut(&broadcast.batch_id)
                .unwrap()
                .push((identity.clone(), broadcast));
            self.payloads.insert(
                ticket.hash(),
                PayloadState::Downloading((identity.clone(), batch_id)),
            );
            // check if this is our broadcast -- if so don't download it (assume caller then calls handle_payload with data)
            if *identity != self.identity {
                return Ok(Some(ticket));
            }
        } else {
            // TODO implement broadcast for train / tiebreak
            error!(
                "broadcast not implemented for committee member {}",
                broadcast.proof.committee
            );
        }

        Ok(None)
    }

    pub(crate) async fn handle_payload(
        &mut self,
        hash: psyche_network::Hash,
        payload: Payload,
    ) -> Result<()> {
        let (from, batch_id) = match self.payloads.get(&hash) {
            Some(PayloadState::Downloading(x)) => x,
            Some(PayloadState::Downloaded(_)) => {
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

        let mut remaining_batch_ids = self.remaining_batch_ids.lock().await;
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
        if remaining_batch_ids.remove(batch_id) {
            self.num_remaining_batch_ids -= 1;
        }
        self.payloads
            .insert(hash, PayloadState::Downloaded(payload));
        if self.num_remaining_batch_ids == 0 {
            self.notify_poll_next.notify_one(); // wake up poll_next() to send opprotunistic witness
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
            match &state.model {
                Some(model) => {
                    self.data_and_model_load = Some(tokio::spawn(State::load_data_and_model(
                        self.identity.clone(),
                        self.private_key.clone(),
                        model.clone(),
                        self.data_parallelism,
                        self.tensor_parallelism,
                    )))
                }
                None => {
                    warn!("Run has no model");
                }
            }
        }
        Ok(())
    }

    async fn round_train(&mut self, index: u64) -> Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round train"))?;
        assert_eq!(state.run_state, RunState::RoundTrain);

        // if all our states are empty (first execution), wait for the data provider and model load to finish
        if self.trainers.is_empty() && self.trainings.is_empty() && self.data_receiver.is_none() {
            let data_and_model_load = self.data_and_model_load.take().ok_or(Error::msg(
                "Round started but no model load was running. Did we miss warmup?",
            ))?;
            if !data_and_model_load.is_finished() {
                bail!("Data and model load not finished when round started!")
            }
            let (data_provider, models) = data_and_model_load.await??; // CANCEL SAFETY POINT
            self.data_receiver = Some(fetch_data(
                data_provider,
                self.notify_new_batch.clone(),
                state.data_indicies_per_batch,
                self.remaining_batch_ids.clone(),
                self.data_parallelism * 2,
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
            self.trainers = models
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
        if !self.trainings.is_empty() {
            bail!("Ready to train but previous training batch still running");
        }

        let round = state.current_round()?;

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            state.clients.len(),
            round.random_seed,
        );

        // let data_ids = assign_data_for_state(&state, &committee_selection)
        //     .iter()
        //     .filter(|(_, v)| **v == self.identity)
        //     .flat_map(|(k, _)| (k.start as usize..k.end as usize + 1).collect::<Vec<_>>())
        //     .collect::<Vec<_>>();
        {
            let mut remaining_batch_ids = self.remaining_batch_ids.lock().await;
            if let Some(data_receiver) = self.data_receiver.as_mut() {
                // drain any pending batches -- this will not loop forever since we are holding the
                // remaining_batch_ids lock, so fetch_data can't push anything new in
                loop {
                    match data_receiver.try_recv() {
                        Ok(_) => {}
                        Err(_) => {
                            break;
                        }
                    }
                }
                let mut batch_ids = get_batch_ids_for_state(state);
                self.num_remaining_batch_ids = batch_ids.len();
                *remaining_batch_ids = batch_ids.drain(0..).collect();
            }
        }
        let committee_proof = committee_selection.get_committee(index);
        let witness_proof = committee_selection.get_witness(index);
        info!(
            "Assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={}",
            state.step, round.height, state.epoch, index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness
        );
        self.blooms = match witness_proof.witness {
            true => {
                let commit_bloom = Bloom::random(
                    self.num_remaining_batch_ids * 2,
                    BLOOM_FALSE_RATE,
                    BLOOM_MAX_BITS,
                );
                let participant_bloom =
                    Bloom::random(state.clients.len(), BLOOM_FALSE_RATE, BLOOM_MAX_BITS);
                let order_bloom = Bloom::random(
                    self.num_remaining_batch_ids,
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
        self.commitments.clear();
        self.commitments_per_client.clear();
        self.payloads.clear();
        self.notify_clear_uploads.notify_one(); // clear any served uploads we have
        self.notify_poll_next.notify_one(); // wake up poll_next() to start data download and training
        self.notify_new_batch.notify_one();

        // if !data_ids.is_empty() {
        //     let data_provider = std::mem::take(&mut self.data_provider)
        //         .ok_or(Error::msg("Round start but no data provider object"))?;
        //     self.fetching_data = Some(tokio::spawn(Self::fetch_data(data_provider, data_ids)));
        //     self.notify.notify_one()
        // } else {
        //     info!(
        //         "No data assigned for round {} of run {}",
        //         round.height, state.run_id
        //     );
        // }
        Ok(())
    }

    fn round_witness(&mut self, index: u64) -> Result<Option<Witness>> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round witness"))?;
        assert_eq!(state.run_state, RunState::RoundWitness);

        Ok(self.get_witness_to_send(index))
    }

    async fn round_apply(&mut self) -> Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round apply"))?;
        assert_eq!(state.run_state, RunState::RoundApply);

        let gpus_still_running = self.data_parallelism - self.trainers.len();
        if gpus_still_running > 0 {
            debug!("Apply round but {gpus_still_running} gpus aren't finished, waiting 1s");
            sleep(Duration::from_secs(1)).await; // CANCEL SAFETY
        }

        // check if this is a state transition
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was witness"))?
            .run_state
            == RunState::RoundApply
        {
            return Ok(());
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
        }

        let round = state.current_round()?;
        let trainers = self.trainers.drain(0..).collect::<Vec<_>>();
        let mut payloads: HashMap<psyche_network::Hash, PayloadState<T>> =
            self.payloads.drain().collect();
        let witnesses = round.witnesses.clone();
        let commitments: HashMap<u64, Vec<(T, BroadcastMessage)>> =
            self.commitments.drain().collect();
        let step = state.step as usize;
        let batch_ids = get_batch_ids_for_state(state);
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
                ) {
                    Some(x) => x,
                    None => {
                        warn!(
                            "DESYNC: Missing consensus commitment for batch {}",
                            batch_id
                        );
                        continue;
                    }
                };
                let consensus = &batch_commitments[consensus].1;
                let payload = match payloads.remove(&consensus.ticket.hash()) {
                    Some(PayloadState::Downloaded(x)) => x,
                    _ => {
                        warn!("DESYNC: Did not finish downloading payload for consensus commitment 0x{} for batch {}", hex::encode(consensus.commitment), batch_id);
                        continue;
                    }
                };
                let maybe_results: Result<Vec<DistroResult>, _> = payload
                    .distro_results
                    .iter()
                    .map(|x| x.try_into())
                    .collect();
                match maybe_results {
                    Ok(results) => {
                        distro_results.push(results);
                    }
                    Err(err) => warn!("DESYNC: Got the following error when deserializing results for commitment 0x{}: {}", hex::encode(consensus.commitment), err),
                }
            }

            let futures: Vec<JoinHandle<Result<Trainer>>> = trainers
                .into_iter()
                .map(|trainer| {
                    let distro_results = distro_results.clone();

                    tokio::task::spawn_blocking(move || {
                        let distro_results: Vec<Vec<DistroResult>> = distro_results
                            .into_iter()
                            .map(|x| {
                                x.into_iter()
                                    .map(|y| DistroResult {
                                        sparse_idx: y.sparse_idx,
                                        sparse_val: y.sparse_val,
                                        xshape: y.xshape,
                                        totalk: y.totalk,
                                    })
                                    .collect()
                            })
                            .collect();
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

        let (_, witness_proof, committee_selection) = self
            .committee_info
            .take()
            .ok_or(Error::msg("No committee info in apply"))?;

        if witness_proof.witness {
            let witnesses = round.witnesses.clone();
            let witness_quorum = state.witness_quorum;
            let clients = state.clients.clone();
            self.health_checking = Some(tokio::task::spawn_blocking(move || {
                let mut checks = HealthChecks::new();
                for (index, client) in clients.into_iter().enumerate() {
                    let proof = committee_selection.get_committee(index as u64);
                    match proof.committee {
                        Committee::Trainer => {
                            if !Coordinator::trainer_healthy_by_witnesses(
                                &client,
                                &witnesses,
                                witness_quorum,
                            ) {
                                checks.push(proof);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(checks)
            }));
        }

        Ok(())
    }

    async fn load_data_and_model(
        identity: T,
        private_key: T::PrivateKey,
        model: model::Model,
        data_parallelism: usize,
        tensor_parallelism: usize,
    ) -> Result<(DataProviderTcpClient<T>, Vec<ParallelModels>)> {
        let model::Model::LLM(llm) = model;
        let data_future = match &llm.data_location {
            model::LLMTrainingDataLocation::Server(data_server) => {
                DataProviderTcpClient::connect(data_server, identity, private_key)
            }
            model::LLMTrainingDataLocation::Local(_) => todo!(),
        };
        let model_future: JoinHandle<Result<Vec<LlamaForCausalLM>>> = match &llm.architecture {
            model::LLMArchitecture::HfLlama => match &llm.checkpoint {
                model::Checkpoint::Hub(hub_repo) => {
                    let hub_repo = hub_repo.clone();
                    tokio::spawn(async move {
                        info!("Downloading {}", hub_repo.repo_id);
                        let repo_files = download_model_repo_async(
                            hub_repo.repo_id.clone(),
                            hub_repo.revision,
                            None,
                            None,
                            None,
                            false,
                        )
                        .await?;
                        info!("Loading {}", hub_repo.repo_id);
                        let mut futures = Vec::with_capacity(data_parallelism * tensor_parallelism);
                        for dp in 0..data_parallelism {
                            let communicator_id = CommunicatorId::new().unwrap();
                            for tp in 0..tensor_parallelism {
                                let tensor_parallelism_world = match tensor_parallelism {
                                    1 => None,
                                    tensor_parallelism => {
                                        Some((communicator_id, tp, tensor_parallelism))
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
                                    )
                                }));
                            }
                        }
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
                        Ok(models)
                    })
                }
            },
        };
        let (data, models) = tokio::join!(data_future, model_future);
        let data = data?;
        let mut tp_models = Vec::new();
        for model in models?? {
            if tp_models
                .last()
                .map(|x: &ParallelModels| x.len() == tensor_parallelism)
                .unwrap_or(true)
            {
                tp_models.push(Vec::with_capacity(tensor_parallelism));
            }
            tp_models.last_mut().unwrap().push(model);
        }
        Ok((data, tp_models))
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
                        proof: witness_proof.clone(),
                        commit_bloom,
                        participant_bloom,
                        order_bloom,
                    });
                }
            }
        }
        None
    }

    fn maybe_write_gradients(&self, payload: &Payload) {
        if let Some(write_gradients_dir) = &self.write_gradients_dir {
            info!("trying to write distro result to disk...");
            if let Err(e) = fs::create_dir_all(write_gradients_dir) {
                warn!("Failed to create write_gradients_dir: {e}");
                return;
            };

            let fname = format!(
                "result-step{}-batch{}.vec-postcard",
                payload.step, payload.batch_id
            );
            let fpath = write_gradients_dir.join(&fname);
            let serialized = match disto_results_to_bytes(&payload.distro_results) {
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
                            error!("Failed to write serialized distro result data {fname}: {e}");
                        }
                    }
                }
            });
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
            run_state: coordinator.map(|x| x.run_state).unwrap_or_default(),
            loss: value.losses.clone(),
            batches_left: value.num_remaining_batch_ids,
        }
    }
}
