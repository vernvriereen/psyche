fn main() {}
#[cfg(test)]
mod tests {
    use psyche_centralized_client::app::{AppBuilder, AppParams};
    use psyche_centralized_server::app::{App as ServerApp, DataServerInfo};
    use psyche_centralized_shared::ClientId;
    use psyche_client::BatchShuffleType;
    use psyche_coordinator::Coordinator;
    use psyche_data_provider::TokenSize;
    use psyche_network::SecretKey;
    use std::path::PathBuf;
    use tokio::{
        select,
        sync::{
            mpsc::{self, Receiver, Sender},
            oneshot,
        },
        time::Duration,
    };
    use tokio_util::sync::CancellationToken;

    const RUN_ID: &str = "test";

    enum QueryMsg {
        QueryClients { respond_to: oneshot::Sender<u32> },
    }

    struct CoordinatorServer {
        inner: ServerApp,
        query_chan_rx: Receiver<QueryMsg>,
    }

    impl CoordinatorServer {
        pub async fn new(query_chan_rx: Receiver<QueryMsg>) -> Self {
            let mut coordinator: Coordinator<ClientId> = Coordinator::default();
            coordinator.run_id = RUN_ID.to_string();

            let data_server_info = DataServerInfo {
                dir: PathBuf::from("./"),
                token_size: TokenSize::TwoBytes,
                seq_len: 2048,
                shuffle_seed: [1; 32],
            };

            let server = ServerApp::new(
                false,
                coordinator,
                Some(data_server_info),
                Some(1234),
                Some(8080),
                None,
                None,
                None,
            )
            .await
            .unwrap();

            Self {
                inner: server,
                query_chan_rx,
            }
        }

        pub async fn handle_message(&mut self, msg: QueryMsg) {
            match msg {
                QueryMsg::QueryClients { respond_to } => {
                    let clients_len = self.inner.get_pending_clients_len();
                    let _ = respond_to.send(clients_len as u32).unwrap();
                }
            }
        }

        pub async fn run(&mut self) {
            loop {
                select! {
                    res = self.inner.run() => res.unwrap(),
                    Some(client_msg) = self.query_chan_rx.recv() => self.handle_message(client_msg).await
                }
            }
        }
    }

    struct CoordinatorServerHandle {
        sender: mpsc::Sender<QueryMsg>,
    }

    impl CoordinatorServerHandle {
        pub async fn new() -> Self {
            let (tx, rx) = mpsc::channel(8);
            let mut server = CoordinatorServer::new(rx).await;
            tokio::spawn(async move { server.run().await });
            Self { sender: tx }
        }

        pub async fn get_clients_len(&self) -> u32 {
            let (send, recv) = oneshot::channel();
            let msg = QueryMsg::QueryClients { respond_to: send };
            let _ = self.sender.send(msg).await;
            recv.await.expect("Actor task has been killed")
        }
    }

    #[tokio::test]
    async fn connect_and_disconnect_nodes() {
        let server_handle = CoordinatorServerHandle::new().await;

        // Client
        let client_app_params = AppParams {
            cancel: CancellationToken::default(),
            private_key: SecretKey::generate(),
            server_addr: "localhost:8080".to_string(),
            tx_tui_state: None,
            run_id: RUN_ID.to_string(),
            data_parallelism: 1,
            tensor_parallelism: 1,
            micro_batch_size: None,
            write_gradients_dir: None,
            p2p_port: Some(10),
            eval_tasks: Vec::new(),
            eval_task_max_docs: None,
            checkpoint_upload_info: None,
            hub_read_token: None,
            wandb_info: None,
            batch_shuffle_type: BatchShuffleType::Fixed([0; 32]),
            optim_stats: None,
            grad_accum_in_fp32: false,
        };

        let client_app_builder = AppBuilder::new(client_app_params);

        tokio::spawn(async { client_app_builder.run().await.unwrap() });
        tokio::time::sleep(Duration::from_secs(1)).await;

        let num_clients = server_handle.get_clients_len().await;

        assert_eq!(num_clients, 1);
    }
}
