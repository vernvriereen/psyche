use psyche_coordinator::RunState;
use testing::{
    client::ClientHandle,
    server::CoordinatorServerHandle,
    test_utils::{assert_with_retries, spawn_clients},
};

#[tokio::test(flavor = "multi_thread")]
async fn connect_single_node() {
    let server_handle = CoordinatorServerHandle::default().await;

    let _client_handle = ClientHandle::default().await;
    let connected_clients = || server_handle.get_clients_len();

    assert_with_retries(connected_clients, 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let server_handle = CoordinatorServerHandle::default().await;

    let _client_handles = spawn_clients(number_of_nodes).await;

    let connected_clients = || server_handle.get_clients_len();
    let run_state = || server_handle.get_run_state();

    assert_with_retries(connected_clients, number_of_nodes as usize).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_waiting_for_members_to_warmup() {
    let init_min_clients = 2;

    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;

    let run_state = || server_handle.get_run_state();
    let connected_clients = || server_handle.get_clients_len();

    assert_with_retries(connected_clients, 0).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;

    let _client_handles = spawn_clients(init_min_clients as usize).await;

    assert_with_retries(connected_clients, 2).await;
    assert_with_retries(run_state, RunState::Warmup).await;
}
