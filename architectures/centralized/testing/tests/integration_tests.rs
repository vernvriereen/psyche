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
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handle = ClientHandle::default(server_port, run_id).await;
    let connected_clients = || server_handle.get_pending_clients_len();

    assert_with_retries(connected_clients, 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let init_min_clients = 15;
    let batches_per_round = 4;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(number_of_nodes, server_port, run_id).await;

    let connected_clients = || server_handle.get_pending_clients_len();
    let run_state = || server_handle.get_run_state();

    assert_with_retries(connected_clients, number_of_nodes).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_warmup() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let batches_per_round = 4;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    let run_state = || server_handle.get_run_state();
    let connected_clients = || server_handle.get_clients_len();

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(connected_clients, 0).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;

    // Clients are spawned

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(init_min_clients as usize, server_port, run_id).await;

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
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    // Clients are spawned and state changes to `Warmup`

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let [_client_1_task, client_2_task]: [ClientHandle; 2] = spawn_clients(2, server_port, run_id)
        .await
        .try_into()
        .unwrap();

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // One client is killed, and now state returns to `WaitingForMembers` since the
    // minimum for starting the round is not reached
    client_2_task.client_handle.abort();
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
    assert_with_retries(|| server_handle.get_clients_len(), 1).await;
    assert_with_retries(|| server_handle.get_pending_clients_len(), 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn state_change_waiting_for_members_to_round_train() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let batches_per_round = 4;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(2, server_port, run_id).await;

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
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(2, server_port, run_id).await;

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

#[tokio::test(flavor = "multi_thread")]
async fn validate_all_clients_participate_in_witness_bloom() {
    // We make sure that the number of clients and the batches per round are the same
    // It is important that the number of clients is not greater than the number of batches per round,
    // since if that is the case, there will be clients that will have no data to train in a given round
    // and they won't appear in the bloom filters, making the test fail
    let init_min_clients = 5;
    let batches_per_round = init_min_clients;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(init_min_clients as usize, server_port, run_id).await;

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
        score += psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(
            &client.id, witnesses,
        );
    });

    let number_of_sent_witnesses = witnesses.len();
    let number_of_seen_clients = score / number_of_sent_witnesses as u32;

    assert_eq!(number_of_seen_clients, clients.len() as u32)
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_node_and_complete_round() {
    let init_min_clients = 2;
    let batches_per_round = 2;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let training_delay = 2;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let [client_1_task, _client_2_task] = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
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

    let _client_handle_3 =
        ClientHandle::new_with_training_delay(server_port, run_id, training_delay).await;

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
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
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

/// A new client attempts to join the network during the RoundTrain phase.
/// The new client should not participate in the current round
/// and should attempt to join the network in the subsequent round.
#[tokio::test(flavor = "multi_thread")]
async fn client_join_in_training() {
    // start a normal run with 2 clients
    let init_min_clients = 2;
    let batches_per_round = 2;
    let witness_nodes = 1;
    let witness_quorum = 1;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await;

    assert_with_retries(
        || server_handle.get_clients_len(),
        init_min_clients as usize,
    )
    .await;

    // execute round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;
    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // spawn new client
    let [new_client_handle] =
        spawn_clients_with_training_delay(1, server_port, run_id, training_delay)
            .await
            .try_into()
            .unwrap();

    // assert new client didnt join the round but is ready in peding clients
    assert_with_retries(|| server_handle.get_pending_clients_len(), 3).await;
    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
    let witnesses = &server_handle.get_rounds().await[0].witnesses;

    // clients spawned in RoundTrain state should not be present in the witnesses
    let mut score = 0;
    let pending_clients = server_handle.get_pending_clients().await;
    pending_clients.iter().for_each(|client| {
        score +=
            psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(client, witnesses);
    });
    assert_eq!(score, init_min_clients);

    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
    // the new client tries to join the network
    // but since the llm checkpoint is Ephemeral
    // it results in an InitRunError::ModelIsEphemeral error
    let error = new_client_handle.client_handle.await.unwrap().unwrap_err();
    assert!(error
        .to_string()
        .contains(&psyche_client::InitRunError::ModelIsEphemeral.to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_node_in_training_and_complete_round() {
    let init_min_clients = 2;
    let batches_per_round = 2;
    // all nodes are witness
    let witness_nodes = 0;
    // we set witness_quorum = 2 witness, as one node will be shutdown
    let witness_quorum = 1;
    let training_delay = 2;
    let server_handle = CoordinatorServerHandle::new(
        init_min_clients,
        batches_per_round,
        witness_nodes,
        witness_quorum,
    )
    .await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let [client_1_task, _client_2_task] = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await
    .try_into()
    .unwrap();

    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;
    tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    let clients = server_handle.get_clients().await;
    assert_eq!(clients.len(), 2);

    // shutdown node 1.
    // this round's workload should be handled entirely by node 2.
    client_1_task.client_handle.abort();

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // assert that just one node parcipate in the witness
    let witnesses = &server_handle.get_rounds().await[0].witnesses;
    let mut score = 0;
    clients.iter().for_each(|client| {
        score += psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(
            &client.id, witnesses,
        );
    });
    assert_eq!(score, 1);
    // since up nodes < init_min_clients
    // the network should return to WaitingForMembers
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
}
