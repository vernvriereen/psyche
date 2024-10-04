use crate::{
    trainer::{TrainOutput, Trainer},
    tui::ClientTUIState,
    BroadcastMessage, Payload,
};
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    get_batch_ids_for_state, model, Committee, CommitteeProof, CommitteeSelection, Coordinator,
    HealthChecks, RunState, Witness, WitnessProof, BLOOM_FALSE_RATE, BLOOM_MAX_BITS,
};
use psyche_core::{bytes_to_hex_string, sha256, Bloom, NodeIdentity};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpClient, TokenizedDataProvider,
};
use psyche_modeling::{DistroResult, LlamaForCausalLM};
use psyche_network::{dummy_blob_ticket, BlobTicket, NetworkEvent};
use psyche_watcher::{Backend, BackendWatcher};
use rand::Rng;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tch::Kind;
use tokio::{sync::Notify, task::JoinHandle};
use tracing::{debug, info, warn};

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
    data_and_model_load: TaskResult<(DataProviderTcpClient<T>, LlamaForCausalLM)>,
    data_provider: Option<DataProviderTcpClient<T>>,
    trainer: Option<Trainer>,
    training: TaskResult<(TrainOutput, u64)>,
    fetching_data: TaskResult<(DataProviderTcpClient<T>, Vec<Vec<i32>>, u64)>,
    applying: TaskResult<Trainer>,
    health_checking: TaskResult<HealthChecks>,
    committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    committments: HashMap<u64, Vec<(T, BroadcastMessage)>>,
    committments_per_client: HashMap<T, u32>,
    payloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    blooms: Option<(Bloom<[u8; 32]>, Bloom<[u8; 32]>, Bloom<[u8; 32]>)>,
    notify: Notify,
    losses: Vec<f32>,
    remaining_batch_ids: HashSet<u64>,
    clear_uploads: Arc<Notify>,
}

impl<T: NodeIdentity> State<T> {
    pub fn new(identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            identity,
            private_key,
            showed_inclusion_message: false,
            data_and_model_load: None,
            data_provider: None,
            trainer: None,
            training: None,
            fetching_data: None,
            applying: None,
            health_checking: None,
            committee_info: None,
            state: None,
            prev_state: None,
            blooms: None,
            committments: HashMap::new(),
            committments_per_client: HashMap::new(),
            payloads: HashMap::new(),
            notify: Notify::new(),
            losses: Vec::new(),
            remaining_batch_ids: HashSet::new(),
            clear_uploads: Arc::new(Notify::new()),
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
        match state.run_state {
            RunState::WaitingForMembers => Ok(None),
            RunState::Warmup => self.warmup().map(|_| None),
            RunState::RoundTrain => self.round_train(position).await.map(|_| None),
            RunState::RoundWitness => {
                return self
                    .round_witness(position)
                    .map(|x| x.map(|y| ToSend::Witness(y)))
            }
            RunState::RoundApply => return self.round_apply().map(|_| None),
        }
    }

    pub fn get_clear_downloads_notification(&self) -> Arc<Notify> {
        self.clear_uploads.clone()
    }

    pub async fn poll_next(&mut self) -> Result<ToSend> {
        if let Some(fetching_data) = &mut self.fetching_data {
            let state = self
                .state
                .as_ref()
                .ok_or(Error::msg("Data fetch running, but no state"))?;
            let (data_provider, data, batch_id) = fetching_data.await??;
            self.fetching_data = None;
            self.data_provider = Some(data_provider);

            let trainer: Trainer = self
                .trainer
                .take()
                .ok_or(Error::msg("Round start but no trainer object (didn't finish training previous round or applying it?)"))?;
            let step: usize = state.step as usize;
            self.training = Some(tokio::task::spawn_blocking(move || {
                Ok((trainer.train(step as usize, data)?, batch_id))
            }));
        } else if let Some(training) = &mut self.training {
            let (output, batch_id) = training.await??;
            self.training = None;
            self.trainer = Some(output.trainer);
            self.losses.push(output.loss);
            if !self.is_run_state(RunState::RoundTrain) {
                return Ok(ToSend::Nothing);
            }
            let (committee_proof, _, _) = self
                .committee_info
                .as_ref()
                .ok_or(Error::msg("Training complete but no self proofs"))?;
            let committment = sha256(self.identity.as_ref());
            let step = output.step as u64;
            let broadcast = BroadcastMessage {
                step,
                batch_id,
                committment,
                ticket: dummy_blob_ticket(),
                proof: *committee_proof,
            };
            let payload = Payload {
                step,
                distro_results: output.distro_results.iter().map(|x| x.into()).collect(),
            };
            return Ok(ToSend::Broadcast((broadcast, payload)));
        } else if let Some(applying) = &mut self.applying {
            let trainer = applying.await??;
            self.applying = None;
            self.trainer = Some(trainer);
        } else if self.is_run_state(RunState::RoundTrain)
            && self.trainer.is_some()
            && self.data_provider.is_some()
            && self.fetching_data.is_none()
            && !self.remaining_batch_ids.is_empty()
        {
            let data_provider = self.data_provider.take().unwrap();
            let batch_id = *self
                .remaining_batch_ids
                .iter()
                .nth(rand::thread_rng().gen_range(0..self.remaining_batch_ids.len()))
                .unwrap();
            let data_indicies_per_batch =
                self.state.as_ref().unwrap().data_indicies_per_batch as u64;
            let start_data_id = (batch_id * data_indicies_per_batch) as usize;
            let data_ids = (start_data_id..(start_data_id + data_indicies_per_batch as usize))
                .collect::<Vec<_>>();
            self.fetching_data = Some(tokio::spawn(async move {
                let (data_provider, data) = Self::fetch_data(data_provider, data_ids).await?;
                Ok((data_provider, data, batch_id))
            }));
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
            && self.remaining_batch_ids.is_empty()
        {
            let (_, witness_proof, _) = self.committee_info.as_ref().unwrap();
            if let Some(witness) = self.get_witness_to_send(witness_proof.index) {
                // send opprotunistic witness
                return Ok(ToSend::Witness(witness));
            } else {
                self.notify.notified().await;
            }
        } else {
            self.notify.notified().await;
        }
        Ok(ToSend::Nothing)
    }

    pub fn process_network_event<B: Backend<T> + 'static>(
        &mut self,
        event: NetworkEvent<BroadcastMessage, Payload>,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<Option<BlobTicket>> {
        debug!("got network event {event:?}");
        match event {
            NetworkEvent::MessageReceived((public_key, message)) => {
                // verify they are who they say they are
                debug!(
                    "Committment {:#?} received from {}",
                    message.committment, public_key
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
                    "Payload {:#?} received from {}",
                    downloaded.hash, downloaded.from
                );
                if let Some(state) = &self.state {
                    if state.step == downloaded.data.step as u32 {
                        self.handle_payload(downloaded.hash, downloaded.data)?;
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
            if *self.committments_per_client.get(identity).unwrap_or(&0)
                >= self
                    .state
                    .as_ref()
                    .map(|x| x.max_batches_per_client)
                    .unwrap_or(0)
            {
                info!(
                    "Maximum commitments received from {}, dropping {:#?}",
                    identity, broadcast.committment
                );
                return Ok(None);
            }

            if witness_proof.witness {
                let (commit_bloom, _, _) = self
                    .blooms
                    .as_mut()
                    .ok_or(Error::msg("We are a witness but no blooms"))?;
                commit_bloom.add(&sha256(&broadcast.committment));
            }
            if !self.committments.contains_key(&broadcast.batch_id) {
                self.committments.insert(broadcast.batch_id, Vec::new());
            }
            let ticket = broadcast.ticket.clone();
            let batch_id = broadcast.batch_id;
            self.committments
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
            todo!();
        }

        Ok(None)
    }

    pub(crate) fn handle_payload(
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
        let committments = match self.committments.get(batch_id) {
            Some(committments) => committments,
            None => {
                info!("No committment for payload from {}", from);
                return Ok(());
            }
        };
        let committment = match committments
            .iter()
            .find(|x| x.0 == *from && x.1.ticket.hash() == hash)
        {
            Some(committment) => &committment.1,
            None => {
                info!("No committment for payload from {}", from);
                return Ok(());
            }
        };
        let (_, witness_proof, _) = self
            .committee_info
            .as_ref()
            .ok_or(Error::msg("Payload message processor has no self proofs"))?;
        if witness_proof.witness {
            let (_, participant_bloom, order_bloom) = self
                .blooms
                .as_mut()
                .ok_or(Error::msg("We are a witness but no blooms"))?;
            participant_bloom.add(&sha256(from.as_ref()));
            if self.remaining_batch_ids.contains(batch_id) {
                // first received payload for this batch id, vote for it in consensus
                order_bloom.add(&sha256(&committment.committment));
            }
        }
        // TODO: verify payload matches committment
        // TODO: verify shape of distro_results
        self.remaining_batch_ids.remove(batch_id);
        self.payloads
            .insert(hash, PayloadState::Downloaded(payload));
        if self.remaining_batch_ids.is_empty() {
            self.notify.notify_one(); // wake up poll_next() to send opprotunistic witness
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
        if self.trainer.is_none() && self.training.is_none() && self.data_provider.is_none() {
            let data_and_model_load = self.data_and_model_load.take().ok_or(Error::msg(
                "Round started but no model load was running. Did we miss warmup?",
            ))?;
            if !data_and_model_load.is_finished() {
                bail!("Data and model load not finished when round started!")
            }
            let (data, model) = data_and_model_load.await??; // CANCEL SAFETY POINT
            self.data_provider = Some(data);

            let config = match &state.model {
                Some(model) => model,
                None => {
                    warn!("Run has no model");
                    return Ok(());
                }
            };
            let model::Model::LLM(llm) = config;
            let _llm = llm.clone();
            self.trainer = Some(Trainer::new(model, llm.lr_schedule.into(), llm.optimizer));
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

        if self.fetching_data.is_some() {
            bail!("Ready to train but previous data fetch still running");
        }
        if self.training.is_some() {
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
        self.remaining_batch_ids = get_batch_ids_for_state(state).drain(0..).collect();
        let committee_proof = committee_selection.get_committee(index);
        let witness_proof = committee_selection.get_witness(index);
        info!(
            "Assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={}",
            state.step, round.height, state.epoch, index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness
        );
        self.blooms = match witness_proof.witness {
            true => {
                let commit_bloom = Bloom::random(
                    self.remaining_batch_ids.len() * 2,
                    BLOOM_FALSE_RATE,
                    BLOOM_MAX_BITS,
                );
                let participant_bloom =
                    Bloom::random(state.clients.len(), BLOOM_FALSE_RATE, BLOOM_MAX_BITS);
                let order_bloom = Bloom::random(
                    self.remaining_batch_ids.len(),
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
        self.committments.clear();
        self.committments_per_client.clear();
        self.payloads.clear();
        self.clear_uploads.notify_one(); // clear any served uploads we have
        self.notify.notify_one(); // wake up poll_next() to start data download and training

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

    fn round_apply(&mut self) -> Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or(Error::msg("No state in round apply"))?;
        assert_eq!(state.run_state, RunState::RoundApply);

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

        if !self.trainer.is_some() {
            bail!("Apply round but trainer isn't ready");
        }
        let round = state.current_round()?;
        let trainer = self.trainer.take().unwrap();
        let mut payloads: HashMap<psyche_network::Hash, PayloadState<T>> =
            self.payloads.drain().collect();
        let witnesses = round.witnesses.clone();
        let committments: HashMap<u64, Vec<(T, BroadcastMessage)>> =
            self.committments.drain().collect();
        let step = state.step as usize;
        let batch_ids = get_batch_ids_for_state(state);

        self.applying = Some(tokio::task::spawn_blocking(move || {
            let mut distro_results: Vec<Vec<DistroResult>> = Vec::new();

            for batch_id in batch_ids {
                let batch_committments = match committments.get(&batch_id) {
                    Some(x) => x,
                    None => {
                        warn!("DESYNC: No committments for batch {}", batch_id);
                        continue;
                    }
                };
                let consensus = match Coordinator::<T>::select_consensus_committment_by_witnesses(
                    &batch_committments
                        .iter()
                        .map(|x| x.1.committment)
                        .collect::<Vec<_>>(),
                    &witnesses,
                ) {
                    Some(x) => x,
                    None => {
                        warn!(
                            "DESYNC: Missing consensus committment for batch {}",
                            batch_id
                        );
                        continue;
                    }
                };
                let consensus = &batch_committments[consensus].1;
                let payload = match payloads.remove(&consensus.ticket.hash()) {
                    Some(PayloadState::Downloaded(x)) => x,
                    _ => {
                        warn!("DESYNC: Did not finish downloading payload for consensus committment {} for batch {}", bytes_to_hex_string(&consensus.committment), batch_id);
                        continue;
                    }
                };
                let maybe_results: Result<Vec<DistroResult>, _> = payload
                    .distro_results
                    .into_iter()
                    .map(|x| x.try_into())
                    .collect();
                match maybe_results {
                    Ok(results) => {
                        distro_results.push(results);
                    }
                    Err(err) => warn!("DESYNC: Got the following error when deserializing results for committment {:}: {}", bytes_to_hex_string(&consensus.committment), err),
                }
            }

            trainer.apply_distro_results(step, distro_results)
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

    async fn fetch_data(
        mut data_provider: DataProviderTcpClient<T>,
        data_ids: Vec<usize>,
    ) -> Result<(DataProviderTcpClient<T>, Vec<Vec<i32>>)> {
        let data = data_provider.get_samples(data_ids).await?;
        Ok((data_provider, data))
    }

    async fn load_data_and_model(
        identity: T,
        private_key: T::PrivateKey,
        model: model::Model,
    ) -> Result<(DataProviderTcpClient<T>, LlamaForCausalLM)> {
        let model::Model::LLM(llm) = model;
        let data_future = match &llm.data_location {
            model::LLMTrainingDataLocation::Server(data_server) => {
                DataProviderTcpClient::connect(data_server, identity, private_key)
            }
            model::LLMTrainingDataLocation::Local(_) => todo!(),
        };
        let model_future = match &llm.architecture {
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
                        let model = tokio::task::spawn_blocking(move || {
                            LlamaForCausalLM::from_pretrained(
                                &repo_files,
                                Some(Kind::BFloat16),
                                None,
                                None,
                            )
                        })
                        .await?;
                        info!("Loading complete {}", hub_repo.repo_id);
                        model
                    })
                }
            },
        };
        let (data, model) = tokio::join!(data_future, model_future);
        Ok((data?, model??))
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
            batches_left: value.remaining_batch_ids.len(),
        }
    }
}
