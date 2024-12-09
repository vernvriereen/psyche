use std::time::Duration;

use psyche_coordinator::RunState;
use testing::{
    server::CoordinatorServerHandle, test_utils::{assert_with_retries, client_app_builder_default_for_testing}, MAX_ROUND_TRAIN_TIME, WARMUP_TIME
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

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_warmup_to_waiting_for_members() {
    let server_handle = CoordinatorServerHandle::new(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_1_task = tokio::spawn(async {
        let client_app_builder_1 = client_app_builder_default_for_testing();
        client_app_builder_1.run().await.unwrap();
    });
    let client_2_task = tokio::spawn(async {
        let client_app_builder_2 = client_app_builder_default_for_testing();
        client_app_builder_2.run().await.unwrap();
    });

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // shutdown client 2
    client_2_task.abort();

    assert_with_retries(|| server_handle.get_clients_len(), 1).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_warmup_to_round_train() {
    let server_handle = CoordinatorServerHandle::new_with_model(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    tokio::spawn(async {
        let client_app_builder_1 = client_app_builder_default_for_testing();
        client_app_builder_1.run().await.unwrap();
    });
    tokio::spawn(async {
        let client_app_builder_2 = client_app_builder_default_for_testing();
        client_app_builder_2.run().await.unwrap();
    });

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // warmup time
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assert_state_change_round_train_to_round_witness() {
    let server_handle = CoordinatorServerHandle::new_with_model(2).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    tokio::spawn(async {
        let client_app_builder_1 = client_app_builder_default_for_testing();
        client_app_builder_1.run().await.unwrap();
    });

    tokio::spawn(async {
        let client_app_builder_2 = client_app_builder_default_for_testing();
        client_app_builder_2.run().await.unwrap();
    });

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // warmup time
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // train time
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME - 1)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
}
