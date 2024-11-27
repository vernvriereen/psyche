use std::time::Duration;

use psyche_centralized_client::app::{AppBuilder, AppParams};
use psyche_client::BatchShuffleType;
use psyche_coordinator::RunState;
use psyche_network::SecretKey;
use testing::server::{CoordinatorServerHandle, RUN_ID};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn connect_single_node() {
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

    // Wait to ensure client is up
    tokio::time::sleep(Duration::from_secs(1)).await;

    let num_clients = server_handle.get_clients_len().await;

    assert_eq!(num_clients, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes: u32 = 10;
    let server_handle = CoordinatorServerHandle::new().await;

    for _ in 0..number_of_nodes {
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
    }
    // Wait to ensure client are up
    tokio::time::sleep(Duration::from_secs(3)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, number_of_nodes);
    assert_eq!(run_state, RunState::WaitingForMembers);
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_waiting_for_members_to_warmup() {
    let server_handle = CoordinatorServerHandle::new_custom(Some(2)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 0);
    assert_eq!(run_state, RunState::WaitingForMembers);

    for _ in 0..2 {
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
    }
    // Wait to ensure client are up
    tokio::time::sleep(Duration::from_secs(2)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 2);
    assert_eq!(run_state, RunState::Warmup);
}
