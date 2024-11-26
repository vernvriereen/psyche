fn main() {
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use psyche_centralized_client::app::{AppBuilder, AppParams};
    use psyche_centralized_server::app::{App as ServerApp, DataServerInfo};
    use psyche_centralized_shared::ClientId;
    use psyche_client::BatchShuffleType;
    use psyche_network::SecretKey;
    use tokio::sync::Mutex;
    use tokio::time::Duration;
    use psyche_coordinator::Coordinator;
    use std::path::PathBuf;
    use psyche_data_provider::TokenSize;
    use tokio_util::sync::CancellationToken;




    #[tokio::test]
    async fn connect_and_disconnect_nodes() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut coordinator: Coordinator<ClientId> = Coordinator::default();

        coordinator.run_id = "test".to_string();

        let p2p_port = Some(10);

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
            // p2p port
            Some(1234),
            Some(8080),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let server = Arc::new(Mutex::new(server));
        let server_clone = server.clone();
        let server_task =
            tokio::spawn(async move { server_clone.lock().await.run().await.unwrap() });

        // Client
        let client_app_params = AppParams {
            cancel: CancellationToken::default(),
            private_key: SecretKey::generate(),
            server_addr: "localhost:8080".to_string(),
            tx_tui_state: None,
            run_id: "test".to_string(),
            data_parallelism: 1,
            tensor_parallelism: 1,
            micro_batch_size: None,
            write_gradients_dir: None,
            p2p_port,
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

        server_task.abort();

        assert_eq!(server.lock().await.get_clients_len(), 1);
    }
}

