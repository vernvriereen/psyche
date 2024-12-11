use std::time::Duration;

use psyche_coordinator::RunState;
use testing::{
    client::ClientHandle,
    server::CoordinatorServerHandle,
    test_utils::{assert_with_retries, spawn_clients},
    MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, WARMUP_TIME,
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
async fn state_change_waiting_for_members_to_warmup() {
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

#[tokio::test(flavor = "multi_thread")]
async fn state_change_shutdown_node_in_warmup() {
    let server_handle = CoordinatorServerHandle::new(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let [_client_1_task, client_2_task] = spawn_clients(2).await.try_into().unwrap();
    // let _client_1_task = tokio::spawn(async {
    //     let client_app_builder_1 = client_app_builder_default_for_testing();
    //     client_app_builder_1.run().await.unwrap();
    // });
    // let client_2_task = tokio::spawn(async {
    //     let client_app_builder_2 = client_app_builder_default_for_testing();
    //     client_app_builder_2.run().await.unwrap();
    // });

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // shutdown client 2
    client_2_task.client_handle.abort();

    assert_with_retries(|| server_handle.get_clients_len(), 1).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_round_train() {
    let server_handle = CoordinatorServerHandle::new_with_model(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // warmup time
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_round_witness() {
    let server_handle = CoordinatorServerHandle::new_with_model(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // warmup time
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // train time
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;

    // Wait for the RoundWitness process to finish.
    // Skipping this wait may cause a deadlock.
    // Issue: https://github.com/NousResearch/psyche/issues/76
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
}
