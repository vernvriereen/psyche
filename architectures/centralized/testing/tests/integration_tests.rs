use std::time::Duration;

use psyche_coordinator::RunState;
use testing::{
    client::ClientHandle,
    server::CoordinatorServerHandle,
    test_utils::{assert_with_retries, spawn_clients},
    COOLDOWN_TIME, MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, WARMUP_TIME,
};

#[tokio::test(flavor = "multi_thread")]
async fn connect_single_node() {
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;

    let server_port = server_handle.server_port;

    let _client_handles = spawn_clients(1, server_port).await;
    let connected_clients = || server_handle.get_pending_clients_len();

    assert_with_retries(connected_clients, 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let init_min_clients = 15;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;

    let server_port = server_handle.server_port;
    let _client_handles = spawn_clients(number_of_nodes, server_port).await;

    let connected_clients = || server_handle.get_pending_clients_len();
    let run_state = || server_handle.get_run_state();

    assert_with_retries(connected_clients, number_of_nodes).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_warmup() {
    let init_min_clients = 2;

    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    let run_state = || server_handle.get_run_state();
    let connected_clients = || server_handle.get_clients_len();

    assert_with_retries(connected_clients, 0).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;

    let _client_handles = spawn_clients(init_min_clients as usize, server_port).await;

    assert_with_retries(connected_clients, 2).await;
    assert_with_retries(run_state, RunState::Warmup).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_shutdown_node_in_warmup() {
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let [_client_1_task, client_2_task]: [ClientHandle; 2] =
        spawn_clients(2, server_port).await.try_into().unwrap();

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
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(2, server_port).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // warmup time
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_round_witness() {
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(2, server_port).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    // warmup time
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    // train time
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
}

/// This test verifies that all clients are included in the witness bloom filters.
/// In rare cases, it may fail due to a bug where the client does not receive
/// the initial peer list from the coordinator, causing it to remain inactive and never start training.
/// If the test fails, it is recommended to rerun it as the issue occurs infrequently.
/// Issue: https://github.com/NousResearch/psyche/issues/89
#[tokio::test(flavor = "multi_thread")]
async fn validate_all_clients_participate_in_witness_bloom() {
    let init_min_clients = 10;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_pending_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(init_min_clients.try_into().unwrap(), server_port).await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;
    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // assert round 0 finished
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // assert that the witness listened all the clients commits
    let witnesses = &server_handle.get_rounds().await[0].witnesses;
    let mut score = 0;
    let clients = server_handle.get_clients().await;
    clients.iter().for_each(|client| {
        score +=
            psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(client, witnesses);
    });
    assert_eq!(score, clients.len() as u32)
}

/// As in the validate_all_clients_participate_in_witness_bloom test,
/// if the test fails, it is recommended to rerun.
/// See issue https://github.com/NousResearch/psyche/issues/89
#[tokio::test(flavor = "multi_thread")]
async fn complete_round_with_shutdown_node() {
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_pending_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    // Spawn two clients
    let [client_1_task,_client_2_task] = spawn_clients(init_min_clients as usize, server_port).await.try_into().unwrap();

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // spawn one more client
    let [_client_3_task] = spawn_clients(1, server_port).await.try_into().unwrap();

    assert_with_retries(|| server_handle.get_pending_clients_len(), 3).await;


    // shutdown client 1
    client_1_task.client_handle.abort();

    // client 3 should replace client 1 a finish the round execution

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;
    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // assert round 0 finished
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // assert that the witness listened all the up clients commits
    let witnesses = &server_handle.get_rounds().await[0].witnesses;
    let mut score = 0;
    let clients = server_handle.get_clients().await;
    assert_eq!(clients.len(), 2);
    clients.iter().for_each(|client| {
        score +=
            psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(client, witnesses);
    });
    assert_eq!(score, clients.len() as u32)
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_epoch() {
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(init_min_clients.try_into().unwrap(), server_port).await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // execute round 1
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // Cooldown
    assert_with_retries(|| server_handle.get_run_state(), RunState::Cooldown).await;
    tokio::time::sleep(Duration::from_secs(COOLDOWN_TIME)).await;

    assert_with_retries(|| server_handle.get_current_epoch(), 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn client_join_in_training() {
    // console_subscriber::init();
    let init_min_clients = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;


    spawn_clients(2, server_port).await;

    assert_with_retries(|| server_handle.get_clients_len(), init_min_clients as usize).await;


    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;
    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    // spawn new client
    spawn_clients(1, server_port).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME - 1)).await;


    // assert new client was not able to join the network
    assert_with_retries(|| server_handle.get_clients_len(), 2).await;




   println!("Rounds: {:?}", server_handle.get_rounds_head().await);
   println!("Clients: {:?}", server_handle.get_clients_len().await);



}
