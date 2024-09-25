use anyhow::Result;
use psyche_coordinator::{select_data_for_state, CommitteeSelection, Coordinator};
use psyche_core::{IntervalTree, NodeIdentity};
use psyche_network::TcpServer;
use psyche_watcher::Backend;
use std::collections::HashMap;
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
    backend: W,
    pub(crate) state: Coordinator<T>,
    pub(crate) selected_data: IntervalTree<u64, T>,
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
            selected_data: IntervalTree::new(),
            provided_sequences: HashMap::new(),
            backend,
            state: Coordinator::default(),
        })
    }

    pub async fn poll(&mut self) {
        tokio::select! {
            new_state = self.backend.wait_for_new_state() => {
                self.handle_new_state(new_state.unwrap());
            }
            Some((from, message)) = self.tcp_server.next() => {
                self.handle_client_message(from, message).await;
            }
        }
    }

    pub async fn handle_client_message(&mut self, from: T, message: ClientToServerMessage) {
        match message {
            ClientToServerMessage::RequestTrainingData { data_ids } => {
                let result = self.try_send_data(from.clone(), data_ids.clone()).await;
                match result {
                    Ok(data) => {
                        for data_id in &data_ids {
                            self.provided_sequences.insert(*data_id, true);
                        }
                        match self
                            .tcp_server
                            .send_to(
                                from.clone(),
                                ServerToClientMessage::TrainingData {
                                    data_ids,
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
                                ServerToClientMessage::RequestRejected { data_ids, reason },
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

    async fn try_send_data(
        &mut self,
        to: T,
        data_ids: Vec<usize>,
    ) -> Result<Vec<Vec<i32>>, RejectionReason> {
        let in_round = self.state.clients.iter().any(|c| c.id == to);
        if !in_round {
            return Err(RejectionReason::NotInThisRound);
        }

        for data_id in &data_ids {
            if self
                .selected_data
                .get(*data_id as u64)
                .is_some_and(|x| *x != to)
            {
                return Err(RejectionReason::WrongDataIdForStep);
            }
        }
        let data = self
            .local_data_provider
            .get_samples(data_ids)
            .await
            .expect("data failed to fetch...");
        Ok(data)
    }

    fn handle_new_state(&mut self, state: Coordinator<T>) {
        self.state = state;
        self.selected_data = match self.state.current_round() {
            Some(round) => {
                let committee = CommitteeSelection::new(
                    round.tie_breaker_tasks as usize,
                    self.state.witness_nodes as usize,
                    self.state.verification_percent,
                    &self.state.clients,
                    round.random_seed,
                );
                select_data_for_state(&self.state, &committee)
            }
            None => IntervalTree::new(),
        };
    }
}
