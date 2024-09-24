use crate::training::Trainer;
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    model, select_data_for_clients, Client, Committee, CommitteeSelection, Coordinator,
    OwnedCommitteeAndWitnessWithProof, Round, RunState,
};
use psyche_core::{ClosedInterval, NodeIdentity};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpClient, TokenizedDataProvider,
};
use psyche_modeling::LlamaForCausalLM;
use tch::Kind;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub(crate) struct State<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    showed_inclusion_message: bool,
    data_and_model_load: Option<JoinHandle<Result<(DataProviderTcpClient<T>, LlamaForCausalLM)>>>,
    data_provider: Option<DataProviderTcpClient<T>>,
    trainer: Option<Trainer>,
    training: Option<JoinHandle<Result<Trainer>>>,
    fetching_data: Option<JoinHandle<Result<(DataProviderTcpClient<T>, Vec<Vec<i32>>)>>>,
    committee_proof: Option<OwnedCommitteeAndWitnessWithProof>,
    state: Option<Coordinator<T>>,
    prev_state: Option<Coordinator<T>>,
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

    pub async fn poll_next(&mut self) -> Result<()> {
        if let Some(fetching_data) = &mut self.fetching_data {
            let state = self.state.as_ref().ok_or(Error::msg("Data finished, but no state"))?;
            let (data_provider, data) = fetching_data.await??;
            self.data_provider = Some(data_provider);
            let model = match &state.model {
                Some(model) => model,
                None => {
                    warn!("Run has no model");
                    return Ok(());
                }
            };
            let model::Model::LLM(llm) = model;
            let _llm = llm.clone();
            let trainer: Trainer = std::mem::take(&mut self.trainer)
                .ok_or(Error::msg("Round start but no trainer object"))?;
            self.training = Some(tokio::spawn(trainer.train(llm.lr_schedule.into(), llm.optimizer, data)));
        }
        Ok(())
    }

    async fn warmup(&mut self) {
        let state = self.state.as_ref().unwrap();
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
        let state = self.state.as_ref().unwrap();
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
            self.trainer = Some(Trainer::new(model));
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
        if self.training.is_some() {
            bail!("Ready to train but previous training batch still running");
        }

        let round = state.current_round().unwrap();

        let committee_proof: OwnedCommitteeAndWitnessWithProof = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            &state.clients,
            round.random_seed,
        )
        .get_selection(&state.clients[position])
        .unwrap()
        .into();
        let committee = committee_proof.committee;
        self.committee_proof = Some(committee_proof);


        let data_ids = match committee {
            Committee::TieBreaker => todo!(),
            Committee::Verifier => todo!(),
            Committee::Trainer => State::get_data_ids(
                &self.identity,
                &state.clients,
                &round,
                state.data_indicies_per_round.into(),
            ),
        };

        let data_ids = data_ids
            .into_iter()
            .flat_map(|x| ((x.start as usize)..(x.end as usize + 1)).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let data_provider = std::mem::take(&mut self.data_provider)
            .ok_or(Error::msg("Round start but no data provider object"))?;
        self.fetching_data = Some(tokio::spawn(Self::fetch_data(data_provider, data_ids)));
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
                DataProviderTcpClient::connect(&data_server, identity, private_key)
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
        return Ok((data?, model??));
    }

    fn get_data_ids(
        identity: &T,
        clients: &[Client<T>],
        round: &Round,
        data_indicies_per_round: u64,
    ) -> Vec<ClosedInterval<u64>> {
        select_data_for_clients(
            clients,
            round.data_index,
            data_indicies_per_round,
            round.random_seed,
        )
        .iter()
        .filter_map(|x| match x.1 == identity {
            true => Some(x.0.clone()),
            false => None,
        })
        .collect::<Vec<_>>()
    }
}
