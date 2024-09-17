use anyhow::Result;
use futures::{SinkExt, StreamExt};
use psyche_coordinator::Coordinator;
use psyche_core::{Networkable, NodeIdentity};
use psyche_watcher::Backend;
use rand::RngCore;
use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info, warn};

use crate::traits::TokenizedDataProvider;

use super::shared::{ChallengeResponse, ServerToClientMessage, TrainingData};

pub struct DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity,
    D: TokenizedDataProvider,
    W: Backend<T>,
{
    clients: Arc<tokio::sync::Mutex<HashMap<T, mpsc::Sender<TrainingData>>>>,
    local_data_provider: D,
    backend: Arc<W>,
}

impl<T, D, W> DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity + 'static,
    D: TokenizedDataProvider + 'static,
    W: Backend<T> + 'static,
{
    pub fn new(local_data_provider: D, backend: W) -> Self {
        DataProviderTcpServer {
            clients: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            local_data_provider,
            backend: Arc::new(backend),
        }
    }

    async fn handle_new_connection(
        stream: TcpStream,
        clients: Arc<tokio::sync::Mutex<HashMap<T, mpsc::Sender<TrainingData>>>>,
    ) {
        let (tx, mut rx) = mpsc::channel(32);
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        let challenge = {
            let mut challenge = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut challenge);
            challenge
        };

        info!("sending challenge to client...");
        if let Err(e) = framed
            .send(
                ServerToClientMessage::Challenge(challenge)
                    .to_bytes()
                    .into(),
            )
            .await
        {
            error!("Failed to send challenge to client: {e}");
            return;
        };
        // Read the first message for identity verification
        let first_message = match framed.next().await {
            Some(Ok(bytes)) => bytes,
            _ => return, // Connection closed or error occurred
        };

        let node_identity = match ChallengeResponse::from_bytes(&first_message) {
            Ok(response) => match T::from_signed_bytes(&response.0, challenge) {
                Ok(identity) => identity,
                Err(e) => {
                    error!("Failed to validate challenge response: {e}");
                    return;
                }
            },
            Err(e) => {
                error!("Failed to deserialize challenge response: {e}");
                return;
            }
        };
        info!("challenge response OK from client {:?}", node_identity);

        clients.lock().await.insert(node_identity.clone(), tx);

        loop {
            tokio::select! {
                Some(data) = rx.recv() => {
                    if framed.send(ServerToClientMessage::TrainingData(data).to_bytes().into()).await.is_err() {
                        break;
                    }
                }
                result = framed.next() => match result {
                    Some(Ok(_)) => {
                        warn!("got non-challenge message from client, killing connection.");
                        break;
                    }
                    _ => break, // Connection closed or error occurred
                },
            }
        }

        info!("connection closed.");

        clients.lock().await.remove(&node_identity);
    }

    pub async fn run(&mut self, port: u16) -> Result<(), anyhow::Error> {
        let addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), port);
        let listener = TcpListener::bind(addr).await?;
        info!("Server listening on: {addr}");

        loop {
            select! {
                    Ok((stream, _)) = listener.accept() => {
                        let clients = self.clients.clone();
                        tokio::spawn(async move {
                            info!("new connection!");
                            Self::handle_new_connection(stream, clients).await;
                        });
                    },
                    new_state = self.backend.wait_for_new_state() => {
                    self.handle_coordinator_update(new_state.clone()).await?;
                }
            }
        }
    }

    async fn handle_coordinator_update(
        &mut self,
        new_state: Coordinator<T>,
    ) -> Result<(), anyhow::Error> {
        let mut connected_clients = self.clients.lock().await;
        let this_round_clients = new_state.clients;
        let to_remove: Vec<T> = connected_clients
            .keys()
            .filter(|connected_id| !this_round_clients.iter().any(|c| &&c.id == connected_id))
            .cloned()
            .collect();

        for key in to_remove {
            info!(
                "connected client {:?} isn't in this new round, closing connection.",
                key
            );
            connected_clients.remove(&key);
        }

        for client in this_round_clients {
            if let Some(conn) = connected_clients.get(&client.id) {
                let data = self.local_data_provider.get_sample(0).await?; // TODO: how to compute data ID?
                conn.send(TrainingData {
                    data_id: 0,
                    raw_data: data,
                })
                .await?;
                info!("sent training data to {:?}", client);
            }
        }

        Ok(())
    }
}
