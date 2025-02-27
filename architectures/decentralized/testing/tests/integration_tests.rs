use std::{sync::Arc, time::Duration};

use bollard::Docker;
use e2e_testing::{
    docker_setup::{e2e_testing_setup, spawn_new_client, CLIENT_CONTAINER_PREFIX},
    docker_watcher::{DockerWatcher, JsonFilter, Response},
};
use psyche_coordinator::{model::Checkpoint, RunState};
use psyche_decentralized_testing::utils::SolanaTestClient;
use serial_test::serial;
use tokio::time;

/// spawn 1 client and run 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_one_client_three_epochs_run() {
    // set test variables
    let run_id = "test".to_string();
    // epochs the test will run
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(1);

    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(timestamp, _client_1, old_state, new_state) => {
                let coordinator_state = solana_client.get_run_state().await;
                println!(
                    "client: new_state: {}, old_state: {}, timestamp: {}",
                    new_state, old_state, timestamp
                );
                // assert client and coordinator state synchronization
                if new_state != RunState::WaitingForMembers.to_string() {
                    assert_eq!(coordinator_state.to_string(), new_state.to_string());
                }
            }

            Response::Loss(client, epoch, step, loss) => {
                println!(
                    "client: {:?}, epoch: {}, step: {}, Loss: {}",
                    client, epoch, step, loss
                );
                // assert that the loss decreases each epoch
                if epoch as i64 > current_epoch {
                    current_epoch = epoch as i64;
                    assert!(loss < last_epoch_loss);
                    last_epoch_loss = loss;
                    if epoch == num_of_epochs_to_run {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
}

// Test p2p model sharing process
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_client_join_and_get_model_p2p() {
    // set test variables
    let run_id = "test".to_string();

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(1);

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(20)).await;

    println!("Adding new client");
    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    spawn_new_client(docker).await.unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![JsonFilter::LoadedModel],
        )
        .unwrap();

    let mut interval = time::interval(Duration::from_secs(20));
    loop {
        tokio::select! {
           _ = interval.tick() => {
                   let current_epoch = solana_client.get_current_epoch().await;
                   println!("Waiting for epoch to finish");
                   if current_epoch >= 1 {
                       panic!("Client couldn't load the model");
               }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::LoadedModel(checkpoint)) = response {
                   // assert client and coordinator state synchronization
                   assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                   assert!(matches!(solana_client.get_checkpoint().await, Checkpoint::P2P(_)), "The coordinator must be on P2P");
                   println!("Client got the model with P2P");
                   return;
               }
           }
        }
    }
}

// Test p2p model sharing process
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_two_client_join_and_get_model_p2p() {
    // set test variables
    let run_id = "test".to_string();

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(1);

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(20)).await;

    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    println!("Adding new first client");
    spawn_new_client(docker.clone()).await.unwrap();

    println!("Adding new second client");
    spawn_new_client(docker).await.unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![JsonFilter::LoadedModel],
        )
        .unwrap();

    let _monitor_client_3 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-3"),
            vec![JsonFilter::LoadedModel],
        )
        .unwrap();

    let mut clients_with_model = 0_u8;
    let mut interval = time::interval(Duration::from_secs(20));
    loop {
        tokio::select! {
           _ = interval.tick() => {
                   let current_epoch = solana_client.get_current_epoch().await;
                   println!("Waiting for epoch to finish");
                   if current_epoch >= 1 {
                    panic!("Client couldn't load the model");
            }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::LoadedModel(checkpoint)) = response {
                   // assert client and coordinator state synchronization
                   assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                   assert!(matches!(solana_client.get_checkpoint().await, Checkpoint::P2P(_)), "The coordinator must be on P2P");
                   clients_with_model += 1;
                   if clients_with_model == 2 {
                        println!("Both new clients got the model with P2P");
                        return;
                   }
               }
           }
        }
    }
}
