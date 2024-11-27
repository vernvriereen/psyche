use std::time::Duration;

use psyche_centralized_client::app::{AppBuilder, AppParams};
use psyche_client::BatchShuffleType;
use psyche_network::SecretKey;
use testing::server::{CoordinatorServerHandle, RUN_ID};
use tokio_util::sync::CancellationToken;

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
