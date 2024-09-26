use crate::{
    trainer::{TrainOutput, Trainer},
    BroadcastMessage, Payload
};
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    model, select_data_for_state, tree_item, Committee, CommitteeSelection, Coordinator,
    OwnedCommitteeAndWitnessWithProof, RunState, COMMITTEE_SALT,
};
use psyche_core::{NodeIdentity, RootType};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpClient, TokenizedDataProvider,
};
use psyche_modeling::LlamaForCausalLM;
use psyche_network::NetworkEvent;
use psyche_watcher::{Backend, BackendWatcher};
use tch::Kind;
use tokio::{sync::Notify, task::JoinHandle};
use tracing::{info, warn};

type TaskResult<T> = Option<JoinHandle<Result<T>>>;

pub struct State<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    showed_inclusion_message: bool,
    data_and_model_load: TaskResult<(DataProviderTcpClient<T>, LlamaForCausalLM)>,
    data_provider: Option<DataProviderTcpClient<T>>,
    trainer: Option<Trainer>,
    training: TaskResult<TrainOutput>,
    fetching_data: TaskResult<(DataProviderTcpClient<T>, Vec<Vec<i32>>)>,
    committee_proof: Option<OwnedCommitteeAndWitnessWithProof>,
    committee_root: RootType,
    witness_root: RootType,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
    notify: Notify,
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
            committee_proof: None,
            state: None,
            prev_state: None,
            notify: Notify::new(),
            committee_root: RootType::default(),
            witness_root: RootType::default(),
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        prev_state: Option<Coordinator<T>>,
    ) -> Result<()> {
        self.state = Some(state.clone());
        self.prev_state = prev_state;
        let position = match state.clients.iter().position(|x| x.id == self.identity) {
            Some(position) => position,
            None => {
                if !self.showed_inclusion_message {
                    info!("Awaiting inclusion in round");
                    self.showed_inclusion_message = true;
                }
                return Ok(());
            }
        };
        match state.run_state {
            RunState::WaitingForMembers => {}
            RunState::Warmup => self.warmup().await,
            RunState::RoundStart => self.round_start(position).await?,
        }
        Ok(())
    }

    pub async fn poll_next(
        &mut self,
    ) -> Result<Option<(OwnedCommitteeAndWitnessWithProof, Payload)>> {
        if self.fetching_data.is_some() {
            let fetching_data = std::mem::take(&mut self.fetching_data).unwrap();
            let state = self
                .state
                .as_ref()
                .ok_or(Error::msg("Data fetch running, but no state"))?;
            let (data_provider, data) = fetching_data.await??;
            self.data_provider = Some(data_provider);

            let trainer: Trainer = std::mem::take(&mut self.trainer)
                .ok_or(Error::msg("Round start but no trainer object"))?;
            self.training = Some(tokio::spawn(trainer.train(state.step as usize, data)));
        } else if self.training.is_some() {
            let training = std::mem::take(&mut self.training).unwrap();
            let output = training.await??;
            self.trainer = Some(output.trainer);
            // TODO DISTRO
            return Ok(Some((
                self.committee_proof
                    .as_ref()
                    .expect("No self committee proof after training")
                    .clone(),
                Payload {
                    step: output.step as u32,
                },
            )));
        } else {
            self.notify.notified().await;
        }
        Ok(None)
    }

    pub async fn process_network_event<B: Backend<T> + 'static>(
        &mut self,
        event: NetworkEvent<BroadcastMessage, Payload>,
        watcher: &BackendWatcher<T, B>,
    ) -> Result<()> {
        match event {
            NetworkEvent::MessageReceived((public_key, message)) => {
                // verify they are who they say they are
                if let Some(state) = &self.state {
                    if state.step == message.step {
                        if let Some(round) = state.current_round() {
                            if let Some(client) =
                                watcher.get_client_for_p2p_public_key(public_key.as_bytes())
                            {
                                if let Some(root) = message.committee.committee_proof.get_root() {
                                    if *root == self.committee_root {
                                        let committee_node = tree_item(
                                            COMMITTEE_SALT.as_bytes(),
                                            round.random_seed,
                                            client.as_ref(),
                                            message.committee.committee_position,
                                        );
                                        if message
                                            .committee
                                            .committee_proof
                                            .verify_item(&committee_node)
                                        {
                                            self.on_broadcast(client, message);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            NetworkEvent::DownloadComplete(_) => todo!(),
        }
        Ok(())
    }

    fn on_broadcast(&mut self, _client: &psyche_coordinator::Client<T>, _message: BroadcastMessage) {
        let committee_proof = self
            .committee_proof
            .as_ref()
            .expect("Broadcast message processor has no self committee proof");
        if committee_proof.committee == Committee::Trainer {
            // TODO: start applying gradients
        }
        if committee_proof.witness {}
    }

    async fn warmup(&mut self) {
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

    async fn round_start(&mut self, position: usize) -> Result<()> {
        let state = self.state.as_ref().expect("No state in round start");
        assert_eq!(state.run_state, RunState::RoundStart);
        if self.trainer.is_none() && self.training.is_none() && self.data_provider.is_none() {
            let data_and_model_load = std::mem::take(&mut self.data_and_model_load).ok_or(
                Error::msg("Round started but no model load was running. Did we miss warmup?"),
            )?;
            if !data_and_model_load.is_finished() {
                bail!("Data and model load not finished when round started!")
            }
            let (data, model) = data_and_model_load.await??;
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
        if self
            .prev_state
            .as_ref()
            .ok_or(Error::msg("First seen state was round state"))?
            .run_state
            == RunState::RoundStart
        {
            return Ok(());
        }
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
            &state.clients,
            round.random_seed,
        );

        let data_ids = select_data_for_state(&state, &committee_selection)
            .iter()
            .filter(|(_, v)| **v == self.identity)
            .flat_map(|(k, _)| (k.start as usize..k.end as usize + 1).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let committee_proof: OwnedCommitteeAndWitnessWithProof = committee_selection
            .get_selection_with_proof(&state.clients[position])
            .expect("Committee missing client")
            .into();
        self.committee_proof = Some(committee_proof);
        self.committee_root = *committee_selection.get_committee_tree_root();
        self.witness_root = *committee_selection.get_witness_tree_root();

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
