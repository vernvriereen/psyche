use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use psyche_core::{Networkable, NodeIdentity};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::info;

use crate::TokenizedDataProvider;

use super::shared::{ChallengeResponse, ServerToClientMessage, TrainingData};

pub struct DataProviderTcpClient<T: NodeIdentity> {
    identity: T,
    private_key: T::PrivateKey,
    stream: Arc<Mutex<Framed<TcpStream, LengthDelimitedCodec>>>,
}

impl<T: NodeIdentity> DataProviderTcpClient<T> {
    pub async fn connect(addr: &str, identity: T, private_key: T::PrivateKey) -> Result<Self> {
        info!("[{:?}] connecting to server {}", identity, addr);
        let stream = TcpStream::connect(addr).await?;
        info!("[{:?}] connected!", identity);
        let framed = Framed::new(stream, LengthDelimitedCodec::new());

        let me = Self {
            identity,
            private_key,
            stream: Arc::new(Mutex::new(framed)),
        };
        me.handle_challenge().await?;
        Ok(me)
    }

    async fn handle_challenge(&self) -> Result<()> {
        info!("[{:?}] waiting for challenge..", self.identity);
        let mut stream = self.stream.lock().await;
        if let Some(Ok(message)) = stream.next().await {
            match ServerToClientMessage::from_bytes(&message) {
                Ok(ServerToClientMessage::Challenge(challenge)) => {
                    info!("[{:?}] got challenge, sending response..", self.identity);
                    let signed_response =
                        self.identity.to_signed_bytes(&self.private_key, challenge);
                    let response = ChallengeResponse(signed_response);
                    stream.send(response.to_bytes().into()).await?;
                    Ok(())
                }
                _ => Err(anyhow!("Unexpected message from server")),
            }
        } else {
            Err(anyhow!("Failed to receive challenge from server"))
        }
    }

    async fn receive_training_data(&self, data_id: usize) -> Result<Vec<i32>> {
        let mut stream = self.stream.lock().await;
        if let Some(Ok(message)) = stream.next().await {
            match ServerToClientMessage::from_bytes(&message) {
                Ok(ServerToClientMessage::TrainingData(TrainingData {
                    data_id: received_id,
                    raw_data,
                })) => {
                    if received_id == data_id {
                        Ok(raw_data)
                    } else {
                        Err(anyhow!("Received data_id does not match requested data_id"))
                    }
                }
                e => Err(anyhow!("Unexpected message from server {:?}", e)),
            }
        } else {
            Err(anyhow!("Failed to receive training data from server"))
        }
    }
}

impl<T: NodeIdentity + Send + Sync> TokenizedDataProvider for DataProviderTcpClient<T> {
    async fn get_sample(&self, data_id: usize) -> Result<Vec<i32>> {
        info!("[{:?}] get sample..", self.identity);
        self.receive_training_data(data_id).await
    }
}
