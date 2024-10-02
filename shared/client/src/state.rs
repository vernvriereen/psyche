use crate::{
    trainer::{TrainOutput, Trainer},
    tui::ClientTUIState,
    BroadcastMessage, Payload,
};
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    model, select_data_for_state, Committee, CommitteeProof, CommitteeSelection, Coordinator,
    RunState, Witness, WitnessProof, BLOOM_FALSE_RATE, BLOOM_MAX_BITS,
};
use psyche_core::{sha256, Bloom, NodeIdentity};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpClient, TokenizedDataProvider,
};
use psyche_modeling::LlamaForCausalLM;
use psyche_network::{dummy_blob_ticket, BlobTicket, NetworkEvent};
use psyche_watcher::{Backend, BackendWatcher};
use std::collections::HashMap;
use tch::Kind;
use tokio::{sync::Notify, task::JoinHandle};
use tracing::{debug, info, warn};

type TaskResult<T> = Option<JoinHandle<Result<T>>>;

enum PayloadState<T: NodeIdentity> {
    Downloading(T),
    #[allow(dead_code)]
    Downloaded(Payload),
}

pub struct State<T: NodeIdentity> {
    pub identity: T,
    private_key: T::PrivateKey,
    showed_inclusion_message: bool,
    data_and_model_load: TaskResult<(DataProviderTcpClient<T>, LlamaForCausalLM)>,
    data_provider: Option<DataProviderTcpClient<T>>,
    trainer: Option<Trainer>,
    training: TaskResult<TrainOutput>,
    fetching_data: TaskResult<(DataProviderTcpClient<T>, Vec<Vec<i32>>)>,
    committee_info: Option<(CommitteeProof, WitnessProof, CommitteeSelection)>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    committments: HashMap<T, BroadcastMessage>,
    payloads: HashMap<psyche_network::Hash, PayloadState<T>>,
    blooms: Option<(Bloom<[u8; 32]>, Bloom<[u8; 32]>)>,
    notify: Notify,
    losses: Vec<f32>,
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
            committee_info: None,
            state: None,
            prev_state: None,
            blooms: None,
            committments: HashMap::new(),
            payloads: HashMap::new(),
            notify: Notify::new(),
            losses: Vec::new(),
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        prev_state: Option<Coordinator<T>>,
    ) -> Result<Option<Witness>> {
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
            RunState::WaitingForMembers => {}
            RunState::Warmup => self.warmup(),
            RunState::RoundTrain => self.round_train(position).await?,
            RunState::RoundWitness => {
                return self.round_witness(position);
            }
            RunState::RoundApply => {}
        }
        Ok(None)
    }

    pub async fn poll_next(&mut self) -> Result<Option<(BroadcastMessage, Payload)>> {
        if let Some(fetching_data) = &mut self.fetching_data {
            let state = self
                .state
                .as_ref()
                .ok_or(Error::msg("Data fetch running, but no state"))?;
            let (data_provider, data) = fetching_data.await??;
            self.fetching_data = None;
            self.data_provider = Some(data_provider);

            let trainer: Trainer = std::mem::take(&mut self.trainer)
                .ok_or(Error::msg("Round start but no trainer object"))?;
            let step: usize = state.step as usize;
            self.training = Some(tokio::task::spawn_blocking(move || {
                trainer.train(step as usize, data)
            }));
        } else if let Some(training) = &mut self.training {
            let output = training.await??;
            self.training = None;
            self.trainer = Some(output.trainer);
            self.losses.push(output.loss);
            // TODO DISTRO
            let (committee_proof, _, _) = self
                .committee_info
                .as_ref()
                .expect("Training complete but no self proofs");
            let committment = sha256(self.identity.as_ref());
            let step = output.step as u64;
            let broadcast = BroadcastMessage {
                step,
                committment,
                ticket: dummy_blob_ticket(),
                proof: *committee_proof,
            };
            let payload = Payload { step };
            return Ok(Some((broadcast, payload)));
        } else {
            self.notify.notified().await;
        }
        Ok(None)
    }

    pub fn process_network_event<B: Backend<T> + 'static>(
        &mut self,
        event: NetworkEvent<BroadcastMessage, Payload>,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<Option<BlobTicket>> {
        match event {
            NetworkEvent::MessageReceived((public_key, message)) => {
                // verify they are who they say they are
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
                    }
                }
            }
            NetworkEvent::DownloadComplete(downloaded) => {
                if let Some(state) = &self.state {
                    if state.step == downloaded.data.step as u32 {
                        self.handle_payload(downloaded.hash, downloaded.data)?;
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
            .expect("Broadcast message processor has no self proofs");
        // verified by process_network_event caller
        if broadcast.proof.committee == Committee::Trainer {
            if self.committments.contains_key(&identity) {
                debug!("Got duplicated committment from {}", identity);
                return Ok(None);
            }

            if witness_proof.witness {
                let (commit_bloom, _) = self
                    .blooms
                    .as_mut()
                    .expect("We are a witness but no blooms");
                commit_bloom.add(&sha256(identity.as_ref()));
            }
            self.committments
                .insert(identity.clone(), broadcast.clone());
            self.payloads.insert(
                broadcast.ticket.hash(),
                PayloadState::Downloading(identity.clone()),
            );
            // check if this is our broadcast -- if so don't download it (assume caller then calls handle_payload with data)
            if *identity != self.identity {
                return Ok(Some(broadcast.ticket));
            }
        }

        Ok(None)
    }

    pub(crate) fn handle_payload(
        &mut self,
        hash: psyche_network::Hash,
        payload: Payload,
    ) -> Result<()> {
        let from = match self.payloads.get(&hash) {
            Some(PayloadState::Downloading(from)) => from,
            Some(PayloadState::Downloaded(_)) => {
                debug!("Duplicate download of {}", hash);
                return Ok(());
            }
            None => {
                debug!("Unknown download {}", hash);
                return Ok(());
            }
        };
        let (_, witness_proof, _) = self
            .committee_info
            .as_ref()
            .expect("Payload message processor has no self proofs");
        if witness_proof.witness {
            let (_, payload_bloom) = self
                .blooms
                .as_mut()
                .expect("We are a witness but no blooms");
            payload_bloom.add(&sha256(from.as_ref()));
        }
        self.payloads
            .insert(hash, PayloadState::Downloaded(payload));
        Ok(())
    }

    fn warmup(&mut self) {
        let state = self.state.as_ref().expect("No state in warmup");
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
    }

    async fn round_train(&mut self, index: u64) -> Result<()> {
        let state = self.state.as_ref().expect("No state in round start");
        assert_eq!(state.run_state, RunState::RoundTrain);

        // if all our states are empty (first execution), wait for the data provider and model load to finish
        if self.trainer.is_none() && self.training.is_none() && self.data_provider.is_none() {
            let data_and_model_load = std::mem::take(&mut self.data_and_model_load).ok_or(
                Error::msg("Round started but no model load was running. Did we miss warmup?"),
            )?;
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

        let round = state.current_round().expect("Round start has no round");

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            state.clients.len(),
            round.random_seed,
        );

        let data_ids = select_data_for_state(&state, &committee_selection)
            .iter()
            .filter(|(_, v)| **v == self.identity)
            .flat_map(|(k, _)| (k.start as usize..k.end as usize + 1).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let committee_proof = committee_selection.get_committee(index);
        let witness_proof = committee_selection.get_witness(index);
        info!(
            "Assignment for step {} (round {}/epoch {}): committee={} witness={}",
            state.step, round.height, state.epoch, committee_proof.committee, witness_proof.witness
        );
        if witness_proof.witness {
            let commit_bloom = Bloom::random(state.clients.len(), BLOOM_FALSE_RATE, BLOOM_MAX_BITS);
            let payload_bloom =
                Bloom::random(state.clients.len(), BLOOM_FALSE_RATE, BLOOM_MAX_BITS);
            info!(
                "Witness bloom size: {} bits, {} keys",
                commit_bloom.bits.len(),
                commit_bloom.keys.len()
            );
            self.blooms = Some((commit_bloom, payload_bloom));
        }
        self.committee_info = Some((committee_proof, witness_proof, committee_selection));
        self.payloads = HashMap::new();

        if !data_ids.is_empty() {
            let data_provider = std::mem::take(&mut self.data_provider)
                .ok_or(Error::msg("Round start but no data provider object"))?;
            self.fetching_data = Some(tokio::spawn(Self::fetch_data(data_provider, data_ids)));
            self.notify.notify_one()
        } else {
            info!(
                "No data assigned for round {} of run {}",
                round.height, state.run_id
            );
        }
        Ok(())
    }

    fn round_witness(&mut self, index: u64) -> Result<Option<Witness>> {
        if let Some((_, witness_proof, _)) = self.committee_info.as_ref() {
            if witness_proof.witness {
                let blooms = std::mem::take(&mut self.blooms);
                if let Some((commit_bloom, payload_bloom)) = blooms {
                    info!("Submitting witness blooms");
                    return Ok(Some(Witness {
                        index,
                        proof: witness_proof.clone(),
                        commit_bloom,
                        payload_bloom,
                    }));
                }
            }
        }
        Ok(None)
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
                        tokio::task::spawn_blocking(move || {
                            LlamaForCausalLM::from_pretrained(
                                &repo_files,
                                Some(Kind::BFloat16),
                                None,
                                None,
                            )
                        })
                        .await?
                    })
                }
            },
        };
        let (data, model) = tokio::join!(data_future, model_future);
        Ok((data?, model??))
    }
}

impl<T: NodeIdentity> From<&State<T>> for ClientTUIState {
    fn from(value: &State<T>) -> Self {
        let coordinator = value.state.as_ref();
        let round = coordinator.and_then(|x| Some(x.current_round_unchecked()));
        let committee = value.committee_info.as_ref().map(|x| x.0.committee);
        ClientTUIState {
            step: coordinator.map(|x| x.step).unwrap_or_default(),
            height: round.map(|x| x.height).unwrap_or_default(),
            committee,
            run_state: coordinator.map(|x| x.run_state).unwrap_or_default(),
            loss: value.losses.clone(),
        }
    }
}
