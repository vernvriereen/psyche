use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::TcpServer;
use psyche_watcher::Backend;
use std::sync::Arc;
use tracing::{info, warn};

use crate::traits::TokenizedDataProvider;

use super::shared::{ClientToServerMessage, RejectionReason, ServerToClientMessage};

pub struct DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity,
    D: TokenizedDataProvider,
    W: Backend<T>,
{
    tcp_server: TcpServer<T, ClientToServerMessage, ServerToClientMessage>,
    local_data_provider: D,
    backend: Arc<W>,
    state: Coordinator<T>,
}

impl<T, D, W> DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity + 'static,
    D: TokenizedDataProvider + 'static,
    W: Backend<T> + 'static,
{
    pub async fn start(local_data_provider: D, backend: W, port: u16) -> Result<Self> {
        let tcp_server = TcpServer::<T, ClientToServerMessage, ServerToClientMessage>::start(
            format!("0.0.0.0:{port}").parse()?,
        )
        .await?;
        Ok(DataProviderTcpServer {
            tcp_server,
            local_data_provider,
            backend: Arc::new(backend),
            state: Coordinator::default(),
        })
    }

    pub async fn poll(&mut self) {
        tokio::select! {
            new_state = self.backend.wait_for_new_state() => {
                self.state = new_state;
            }
            Some((from, message)) = self.tcp_server.next() => {
                self.handle_client_message(from, message).await;
            }
        }
    }

    pub async fn handle_client_message(&mut self, from: T, message: ClientToServerMessage) {
        match message {
            ClientToServerMessage::RequestTrainingData { data_id } => {
                let in_round = self.state.clients.iter().any(|c| c.id == from);
                if !in_round {
                    self.tcp_server
                        .send_to(
                            from.clone(),
                            ServerToClientMessage::RequestRejected {
                                data_id,
                                reason: RejectionReason::NotInThisRound,
                            },
                        )
                        .await
                        .unwrap();
                }

                let current_data_id_for_client = data_id; // TODO... compute me.
                if current_data_id_for_client != data_id {
                    self.tcp_server
                        .send_to(
                            from.clone(),
                            ServerToClientMessage::RequestRejected {
                                data_id,
                                reason: RejectionReason::WrongDataIdForStep,
                            },
                        )
                        .await
                        .unwrap();
                }
                let data = self
                    .local_data_provider
                    .get_sample(0)
                    .await
                    .expect("data failed to fetch..."); // TODO: how to compute data ID?
                match self
                    .tcp_server
                    .send_to(
                        from.clone(),
                        ServerToClientMessage::TrainingData {
                            data_id: 0,
                            raw_data: data,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        info!("sent training data to {:?}", from);
                    }
                    Err(err) => {
                        warn!("Failed to send training data to {:?}: {err}", from);
                    }
                }
            }
        }
    }
}
