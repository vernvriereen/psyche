use std::time::Duration;

use psyche_coordinator::RunState;
use testing::{
    client::ClientHandle,
    server::CoordinatorServerHandle,
    test_utils::{assert_with_retries, spawn_clients, spawn_clients_with_training_delay},
    COOLDOWN_TIME, MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, WARMUP_TIME,
};

#[tokio::test(flavor = "multi_thread")]
async fn connect_single_node() {
    let init_min_clients = 2;
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    let server_port = server_handle.server_port;
    let _client_handle = ClientHandle::default(server_port).await;
    let connected_clients = || server_handle.get_clients_len();

    assert_with_retries(connected_clients, 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let init_min_clients = 15;
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    let server_port = server_handle.server_port;
    let _client_handles = spawn_clients(number_of_nodes, server_port).await;

    let connected_clients = || server_handle.get_clients_len();
    let run_state = || server_handle.get_run_state();

    assert_with_retries(connected_clients, number_of_nodes).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_warmup() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    let run_state = || server_handle.get_run_state();
    let connected_clients = || server_handle.get_clients_len();

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(connected_clients, 0).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;

    // Clients are spawned

    let server_port = server_handle.server_port;
    let _client_handles = spawn_clients(init_min_clients as usize, server_port).await;

    // Clients have connected and now that the initial min clients has been reached, run state
    // changes to `Warmup`

    assert_with_retries(connected_clients, 2).await;
    assert_with_retries(run_state, RunState::Warmup).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_shutdown_node_in_warmup() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    // Clients are spawned and state changes to `Warmup`

    let server_port = server_handle.server_port;
    let [_client_1_task, client_2_task]: [ClientHandle; 2] =
        spawn_clients(2, server_port).await.try_into().unwrap();

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // One client is killed, and now state returns to `WaitingForMembers` since the
    // minimum for starting the round is not reached

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
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
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
    let batches_per_round = 4;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;
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
    // We make sure that the number of clients and the batches per round are the same
    // It is important that the number of clients is not greater than the number of batches per round,
    // since if that is the case, there will be clients that will have no data to train in a given round
    // and they won't appear in the bloom filters, making the test to fail
    let init_min_clients = 5;
    let batches_per_round = init_min_clients;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;
    let server_port = server_handle.server_port;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let _client_handles = spawn_clients(init_min_clients as usize, server_port).await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // Start round 0

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

    // Assert that the witness listened all the clients commitments from the previous round

    // We get the list of received witnesses from round 0
    let witnesses = &server_handle.get_rounds().await[0].witnesses;

    let mut score = 0;
    let clients = server_handle.get_clients().await;
    clients.iter().for_each(|client| {
        score +=
            psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(client, witnesses);
    });

    let number_of_sent_witnesses = witnesses.len();
    let number_of_seen_clients = score / number_of_sent_witnesses as u32;

    assert_eq!(number_of_seen_clients, clients.len() as u32)
}

#[tokio::test(flavor = "multi_thread")]
async fn complete_round_with_shutdown_node() {
    let init_min_clients = 2;
    let batches_per_round = 2;
    let training_delay = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let [client_1_task, _client_2_task] =
        spawn_clients_with_training_delay(init_min_clients as usize, server_port, training_delay)
            .await
            .try_into()
            .unwrap();

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // A new client is spawned, but since we are in `Warmup` state, it should wait for `WaitingForMembers`
    // to join the run

    let _client_handle_3 = ClientHandle::new_with_training_delay(server_port, training_delay).await;

    // A client is killed and the coordinator state returns to `WaitingForMembers`. Since client 3
    // was pending, the state immediately changes to `Warmup` again
    client_1_task.client_handle.abort();

    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    // The network advances normally

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(training_delay)).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_epoch() {
    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 2;
    let batches_per_round = 2;
    let server_handle = CoordinatorServerHandle::new(init_min_clients, batches_per_round).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let _client_handles =
        spawn_clients_with_training_delay(init_min_clients as usize, server_port, training_delay)
            .await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;

    // Witnesses should be empty, since round just started and we haven't trained yet
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(training_delay)).await;

    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // execute round 1
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // Cooldown
    assert_with_retries(|| server_handle.get_run_state(), RunState::Cooldown).await;
    tokio::time::sleep(Duration::from_secs(COOLDOWN_TIME)).await;

    assert_with_retries(|| server_handle.get_current_epoch(), 1).await;
}
