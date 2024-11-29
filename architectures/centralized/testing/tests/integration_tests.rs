use psyche_coordinator::RunState;
use testing::{
    server::CoordinatorServerHandle,
    test_utils::{assert_with_retries, client_app_builder_default_for_testing},
};

#[tokio::test(flavor = "multi_thread")]
async fn connect_single_node() {
    let server_handle = CoordinatorServerHandle::default().await;

    let client_app_builder = client_app_builder_default_for_testing();
    tokio::spawn(async { client_app_builder.run().await.unwrap() });

    assert_with_retries(|| server_handle.get_clients_len(), 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let server_handle = CoordinatorServerHandle::default().await;

    for _ in 0..number_of_nodes {
        let client_app_builder = client_app_builder_default_for_testing();
        tokio::spawn(async { client_app_builder.run().await.unwrap() });
    }

    assert_with_retries(|| server_handle.get_clients_len(), number_of_nodes as usize).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_waiting_for_members_to_warmup() {
    let init_min_clients = 2;

    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    for _ in 0..2 {
        let client_app_builder = client_app_builder_default_for_testing();
        tokio::spawn(async { client_app_builder.run().await.unwrap() });
    }

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
}
