use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::{ClientNotification, TcpServer};
use psyche_watcher::Backend;
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

use crate::traits::{LengthKnownDataProvider, TokenizedDataProvider};

use super::shared::{ClientToServerMessage, RejectionReason, ServerToClientMessage};

pub struct DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity,
    D: TokenizedDataProvider + LengthKnownDataProvider,
    W: Backend<T>,
{
    tcp_server: TcpServer<T, ClientToServerMessage, ServerToClientMessage>,
    pub(crate) local_data_provider: D,
    backend: W,
    pub(crate) state: Coordinator<T>,
    // pub(crate) selected_data: IntervalTree<u64, T>,
    pub(crate) in_round: HashSet<T>,
    pub(crate) provided_sequences: HashMap<T, usize>,
}

impl<T, D, W> DataProviderTcpServer<T, D, W>
where
    T: NodeIdentity + 'static,
    D: TokenizedDataProvider + LengthKnownDataProvider + 'static,
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
            // selected_data: IntervalTree::new(),
            in_round: HashSet::new(),
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
            Some(event) = self.tcp_server.next() => {
                match event {
                    ClientNotification::Message((from, message)) => {
                        self.handle_client_message(from, message).await;
                    }
                    ClientNotification::Disconnected(_) => {
                        // noop :)
                    }
                }
            }
        }
    }
    pub async fn handle_client_message(&mut self, from: T, message: ClientToServerMessage) {
        match message {
            ClientToServerMessage::RequestTrainingData { data_ids } => {
                let result = self.try_send_data(from.clone(), data_ids.clone()).await;
                match result {
                    Ok(data) => {
                        let old_count = *self.provided_sequences.get(&from).unwrap_or(&0);
                        self.provided_sequences
                            .insert(from.clone(), old_count + data_ids.len());
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
                                debug!("sent training data to {:?}", from);
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
                                debug!("sent error to {:?}", from);
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
        if !self.in_round.contains(&to) {
            return Err(RejectionReason::NotInThisRound);
        }

        // for data_id in &data_ids {
        //     if self
        //         .selected_data
        //         .get(*data_id as u64)
        //         .is_some_and(|x| *x != to)
        //     {
        //         return Err(RejectionReason::WrongDataIdForStep);
        //     }
        // }
        let data = self
            .local_data_provider
            .get_samples(&data_ids)
            .await
            .expect("data failed to fetch...");
        Ok(data)
    }

    fn handle_new_state(&mut self, state: Coordinator<T>) {
        self.state = state;
        self.in_round = self.state.clients.iter().map(|x| x.id.clone()).collect();
        // self.selected_data = match self.state.current_round() {
        //     Ok(round) => {
        //         let committee = CommitteeSelection::new(
        //             round.tie_breaker_tasks as usize,
        //             self.state.witness_nodes as usize,
        //             self.state.verification_percent,
        //             self.state.clients.len(),
        //             round.random_seed,
        //         );
        //         assign_data_for_state(&self.state, &committee)
        //     }
        //     Err(_) => IntervalTree::new(),
        // };
    }
}
