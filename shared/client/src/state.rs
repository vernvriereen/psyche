use crate::training::Trainer;
use anyhow::{bail, Error, Result};
use psyche_coordinator::{model, CommitteeSelection, Coordinator, RunState};
use psyche_core::NodeIdentity;
use psyche_data_provider::{download_model_repo_async, DataProviderTcpClient};
use psyche_modeling::LlamaForCausalLM;
use tch::Kind;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub(crate) struct State<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    active_client: bool,
    data_and_model_load: Option<JoinHandle<Result<(DataProviderTcpClient<T>, LlamaForCausalLM)>>>,
    trainer: Option<Trainer<T>>,
    training: Option<JoinHandle<Trainer<T>>>,
}

impl<T: NodeIdentity> State<T> {
    pub fn new(identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            identity,
            private_key,
            active_client: true,
            data_and_model_load: None,
            trainer: None,
            training: None,
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        prev_state: Option<Coordinator<T>>,
    ) -> Result<()> {
        let active_client = state
            .clients
            .iter()
            .position(|x| x.id == self.identity)
            .is_some();
        if !active_client {
            if self.active_client {
                info!("Awaiting inclusion in round");
                self.active_client = false;
            }
            return Ok(());
        }
        match state.run_state {
            RunState::WaitingForMembers => {}
            RunState::Warmup => self.warmup(state, prev_state).await,
            RunState::RoundStart => self.round_start(state, prev_state).await?,
        }
        Ok(())
    }

    async fn warmup(&mut self, state: &Coordinator<T>, prev_state: Option<Coordinator<T>>) {
        assert_eq!(state.run_state, RunState::Warmup);
        if prev_state.is_none()
            || prev_state
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

    async fn round_start(
        &mut self,
        state: &Coordinator<T>,
        prev_state: Option<Coordinator<T>>,
    ) -> Result<()> {
        assert_eq!(state.run_state, RunState::RoundStart);
        if self.trainer.is_none() {
            assert!(self.training.is_none());
            let data_and_model_load = std::mem::take(&mut self.data_and_model_load).ok_or(
                Error::msg("Round started but no model load was running. Did we miss warmup?"),
            )?;
            if !data_and_model_load.is_finished() {
                bail!("Data and model load not finished when round started!")
            }
            let (data, model) = data_and_model_load.await??;
            self.trainer = Some(Trainer::new(data, model));
        }
        if prev_state
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
        let _committee = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.witness_nodes as usize,
            state.verification_percent,
            &state.clients,
            round.random_seed,
        );
        let model = match &state.model {
            Some(model) => model,
            None => {
                warn!("Run has no model");
                return Ok(());
            }
        };
        let trainer = std::mem::take(&mut self.trainer)
            .ok_or(Error::msg("Round start but no trainer object"))?;
        let model::Model::LLM(llm) = model;
        let llm = llm.clone();
        self.training = Some(tokio::task::spawn_blocking(|| trainer.train(llm)));
        Ok(())
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
}
