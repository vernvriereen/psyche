use std::time::Duration;

use psyche_coordinator::RunState;
use testing::{
    client_test_utils::client_app_builder_default_for_testing, server::CoordinatorServerHandle,
};

#[tokio::test]
async fn connect_single_node() {
    let server_handle = CoordinatorServerHandle::default().await;

    let client_app_builder = client_app_builder_default_for_testing();
    tokio::spawn(async { client_app_builder.run().await.unwrap() });

    // Wait to ensure client is up
    tokio::time::sleep(Duration::from_millis(500)).await;

    let num_clients = server_handle.get_clients_len().await;

    assert_eq!(num_clients, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let server_handle = CoordinatorServerHandle::default().await;

    for _ in 0..number_of_nodes {
        let client_app_builder = client_app_builder_default_for_testing();
        tokio::spawn(async { client_app_builder.run().await.unwrap() });
    }
    // Wait to ensure client are up
    tokio::time::sleep(Duration::from_millis(150 * number_of_nodes)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients as u64, number_of_nodes);
    assert_eq!(run_state, RunState::WaitingForMembers);
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_waiting_for_members_to_warmup() {
    let init_min_clients = 2;

    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 0);
    assert_eq!(run_state, RunState::WaitingForMembers);

    for _ in 0..2 {
        let client_app_builder = client_app_builder_default_for_testing();
        tokio::spawn(async { client_app_builder.run().await.unwrap() });
    }
    // Wait to ensure client are up
    tokio::time::sleep(Duration::from_millis(500)).await;

    let num_clients = server_handle.get_clients_len().await;
    let run_state = server_handle.get_run_state().await;

    assert_eq!(num_clients, 2);
    assert_eq!(run_state, RunState::Warmup);
}
