use std::time::Duration;

use psyche_centralized_client::app::{AppBuilder, AppParams};
use psyche_client::BatchShuffleType;
use psyche_coordinator::RunState;
use psyche_network::SecretKey;
use testing::server::{CoordinatorServerHandle, RUN_ID};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn connect_single_node() {
    let server_handle = CoordinatorServerHandle::default().await;

    let client_app_builder = AppBuilder::default();
    tokio::spawn(async { client_app_builder.run().await.unwrap() });

    // Wait to ensure client is up
    tokio::time::sleep(Duration::from_secs(1)).await;

    let num_clients = server_handle.get_clients_len().await;

    assert_eq!(num_clients, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes: u32 = 10;
    let server_handle = CoordinatorServerHandle::default().await;

    for _ in 0..number_of_nodes {
        let client_app_builder = AppBuilder::default();
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
    let server_handle = CoordinatorServerHandle::new(Some(2)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 0);
    assert_eq!(run_state, RunState::WaitingForMembers);

    for _ in 0..2 {
        let client_app_builder = AppBuilder::default();
        tokio::spawn(async { client_app_builder.run().await.unwrap() });
    }
    // Wait to ensure client are up
    tokio::time::sleep(Duration::from_secs(2)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 2);
    assert_eq!(run_state, RunState::Warmup);
}
