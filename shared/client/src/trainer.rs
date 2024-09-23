use anyhow::Result;
use psyche_coordinator::{
    model::{Checkpoint, LLMArchitecture, LLMTrainingDataLocation, Model},
    Coordinator, RunState,
};
use psyche_core::NodeIdentity;
use psyche_data_provider::{download_model_repo_async, DataProviderTcpClient};
use psyche_modeling::LlamaForCausalLM;
use tch::Kind;
use tracing::info;

pub(crate) struct Trainer<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    data: Option<DataProviderTcpClient<T>>,
    model: Option<LlamaForCausalLM>,
    active_client: bool,
}

impl<T: NodeIdentity> Trainer<T> {
    pub fn new(identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            identity,
            private_key,
            data: None,
            model: None,
            active_client: true,
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
        if self.active_client && !active_client {
            info!("Awaiting inclusion in round");
            self.active_client = false;
            return Ok(());
        }
        if prev_state.is_none()
            || prev_state
                .as_ref()
                .is_some_and(|x| x.run_state != state.run_state)
        {
            match state.run_state {
                RunState::WaitingForMembers => {}
                RunState::Warmup => self.warmup(state).await?,
                RunState::RoundStart => todo!(),
            }
        }
        Ok(())
    }

    async fn warmup(&mut self, state: &Coordinator<T>) -> Result<()> {
        if let Some(Model::LLM(llm)) = &state.model {
            let data_future = match &llm.data_location {
                LLMTrainingDataLocation::Server(data_server) => DataProviderTcpClient::connect(
                    &data_server,
                    self.identity.clone(),
                    self.private_key.clone(),
                ),
                LLMTrainingDataLocation::Local(_) => todo!(),
            };
            let model_future = match &llm.architecture {
                LLMArchitecture::HfLlama => match &llm.checkpoint {
                    Checkpoint::Hub(hub_repo) => {
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
            self.data = Some(data?);
            self.model = Some(model??);
        }
        Ok(())
    }
}
