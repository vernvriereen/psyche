use anyhow::{bail, Result};
use psyche_core::NodeIdentity;
use psyche_network::TcpClient;
use tracing::info;

use crate::TokenizedDataProvider;

use super::shared::{ClientToServerMessage, ServerToClientMessage};

pub struct DataProviderTcpClient<T: NodeIdentity> {
    address: String,
    tcp_client: TcpClient<T, ClientToServerMessage, ServerToClientMessage>,
}

impl<T: NodeIdentity> DataProviderTcpClient<T> {
    pub async fn connect(addr: &str, identity: T, private_key: T::PrivateKey) -> Result<Self> {
        let tcp_client = TcpClient::<T, ClientToServerMessage, ServerToClientMessage>::connect(
            addr,
            identity,
            private_key,
        )
        .await?;
        Ok(Self { tcp_client, address: addr.to_owned() })
    }

    async fn receive_training_data(&mut self, data_id: usize) -> Result<Vec<i32>> {
        self.tcp_client
            .send(ClientToServerMessage::RequestTrainingData { data_id })
            .await?;

        let message = self.tcp_client.receive().await?;
        match message {
            ServerToClientMessage::TrainingData {
                data_id: received_id,
                raw_data,
            } => {
                if received_id == data_id {
                    Ok(raw_data)
                } else {
                    bail!("Received data_id does not match requested data_id")
                }
            }
            e => bail!("Unexpected message from server {:?}", e),
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

impl<T: NodeIdentity + Send + Sync> TokenizedDataProvider for DataProviderTcpClient<T> {
    async fn get_sample(&mut self, data_id: usize) -> Result<Vec<i32>> {
        info!("[{:?}] get sample..", self.tcp_client.get_identity());
        self.receive_training_data(data_id).await
    }
}
