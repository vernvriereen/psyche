use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::TcpServer;
use psyche_watcher::Backend;
use std::{collections::HashMap, sync::Arc};
use tracing::{info, warn};

use crate::traits::TokenizedDataProvider;

use super::shared::{ClientToServerMessage, RejectionReason, ServerToClientMessage};

pub struct DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity,
    D: TokenizedDataProvider,
    W: Backend<T>,
{
    pub(crate) tcp_server: TcpServer<T, ClientToServerMessage, ServerToClientMessage>,
    pub(crate) local_data_provider: D,
    pub(crate) backend: Arc<W>,
    pub(crate) state: Coordinator<T>,
    pub(crate) provided_sequences: HashMap<usize, bool>,
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
            provided_sequences: HashMap::new(),
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
                let result = self.try_send_data(from.clone(), data_id).await;
                match result {
                    Ok(data) => {
                        self.provided_sequences.insert(data_id, true);
                        match self
                            .tcp_server
                            .send_to(
                                from.clone(),
                                ServerToClientMessage::TrainingData {
                                    data_id,
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
                    Err(reason) => {
                        match self
                            .tcp_server
                            .send_to(
                                from.clone(),
                                ServerToClientMessage::RequestRejected { data_id, reason },
                            )
                            .await
                        {
                            Ok(()) => {
                                info!("sent error to {:?}", from);
                            }
                            Err(err) => {
                                warn!("Failed to send error to {:?}: {err}", from);
                            }
                        }
                    }
                }
            }
        }
    }
    async fn try_send_data(&mut self, to: T, data_id: usize) -> Result<Vec<i32>, RejectionReason> {
        let in_round = self.state.clients.iter().any(|c| c.id == to);
        if !in_round {
            return Err(RejectionReason::NotInThisRound);
        }

        let current_data_id_for_client = match self.state.data_id(&to) {
            Some(id) => id,
            None => {
                return Err(RejectionReason::NotInThisRound);
            }
        };
        if current_data_id_for_client != data_id {
            return Err(RejectionReason::WrongDataIdForStep);
        }
        let data = self
            .local_data_provider
            .get_sample(current_data_id_for_client)
            .await
            .expect("data failed to fetch...");
        Ok(data)
    }
}
