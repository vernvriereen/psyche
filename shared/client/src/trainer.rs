use anyhow::Result;
use psyche_coordinator::{
    model::{LLMTrainingDataLocation, Model},
    Coordinator,
};
use psyche_core::NodeIdentity;
use psyche_data_provider::DataProviderTcpClient;

pub(crate) struct Trainer<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    data: Option<DataProviderTcpClient<T>>,
}

impl<T: NodeIdentity> Trainer<T> {
    pub fn new(identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            identity,
            private_key,
            data: None,
        }
    }

    pub async fn process_new_state(
        &mut self,
        state: &Coordinator<T>,
        _prev_state: Option<Coordinator<T>>,
    ) -> Result<()> {
        if let Some(Model::LLM(llm)) = &state.model {
            match &llm.data_location {
                LLMTrainingDataLocation::Server(data_server) => {
                    if match &self.data {
                        Some(data) => data.address() != data_server,
                        None => true,
                    } {
                        self.data = Some(
                            DataProviderTcpClient::connect(
                                &data_server,
                                self.identity.clone(),
                                self.private_key.clone(),
                            )
                            .await?,
                        );
                    };
                }
                LLMTrainingDataLocation::Local(_) => todo!(),
            }
        }
        Ok(())
    }
}
