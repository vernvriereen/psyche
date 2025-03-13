use std::{path::PathBuf, sync::Arc, time::Duration};

use bollard::{container::KillContainerOptions, Docker};
use psyche_coordinator::{model::Checkpoint, RunState};
use psyche_decentralized_testing::{
    chaos::{ChaosAction, ChaosScheduler},
    docker_setup::{
        e2e_testing_setup, kill_all_clients, spawn_new_client, spawn_new_client_with_monitoring,
    },
    docker_watcher::{DockerWatcher, JsonFilter, Response},
    utils::SolanaTestClient,
    CLIENT_CONTAINER_PREFIX, VALIDATOR_CONTAINER_PREFIX,
};
use rstest::*;
use serial_test::serial;
use tokio::time;

/// spawn 2 clients and run for 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_two_clients_three_epochs_run() {
    // set test variables
    let run_id = "test".to_string();

    // epochs the test will run
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(
        docker.clone(),
        2,
        Some(PathBuf::from(
            "../../config/solana-test/light-two-min-clients.toml",
        )),
    )
    .await;

    // Monitor the client container
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    // Initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;
    let mut live_interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health(2).await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state)) => {
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
                    Some(Response::Loss(client, epoch, step, loss)) => {
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
                    _ => unreachable!(),
                }
            }
        }
    }
}

// Test p2p model sharing process
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_client_join_and_get_model_p2p(#[values(1, 2)] n_new_clients: u8) {
    // Test variables
    let run_id = "test".to_string();

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1, None).await;

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(40)).await;

    println!("Adding new clients");
    for i in 1..=n_new_clients {
        spawn_new_client(docker.clone()).await.unwrap();
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i + 1),
                vec![JsonFilter::LoadedModel],
            )
            .unwrap();
    }

    let solana_client = SolanaTestClient::new(run_id).await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    let mut clients_with_model = 0;

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               println!("Waiting for epoch to end");
                if let Err(e) = watcher.monitor_clients_health(n_new_clients).await {
                    panic!("{}", e);
               }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::LoadedModel(checkpoint)) = response {
                   // assert client and coordinator state synchronization
                   assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                   assert!(matches!(solana_client.get_checkpoint().await, Checkpoint::P2P(_)), "The coordinator must be on P2P");

                   clients_with_model += 1;
                   if clients_with_model == n_new_clients {
                       println!("All clients got the model with P2P");
                       return;
                   }
               }
           }
        }
    }
}

#[ignore = "These tests are a bit flaky, so we need to make sure they work properly."]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_pause_solana_validator(
    #[values(1, 2)] n_clients: u8,
    #[values(0, 10)] pause_step: u64,
) {
    // Test variables
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize docker watcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with n_clients clients
    let _cleanup = if n_clients == 1 {
        e2e_testing_setup(docker.clone(), 1, None).await
    } else {
        e2e_testing_setup(
            docker.clone(),
            2,
            Some(PathBuf::from(
                "../../config/solana-test/light-two-min-clients.toml",
            )),
        )
        .await
    };

    // Solana client
    let solana_client = SolanaTestClient::new(run_id).await;

    // Monitor clients
    for i in 1..=n_clients {
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i),
                vec![JsonFilter::Loss],
            )
            .unwrap();
    }

    // Sleep to let the coordinator to be deployed and run to be configured
    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = vec![format!("{VALIDATOR_CONTAINER_PREFIX}-1")];

    let chaos_scheduler = ChaosScheduler::new(docker.clone(), solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Pause {
                duration_secs: 60,
                targets: chaos_targets.clone(),
            },
            pause_step,
        )
        .await;

    // let mut chaos_already_executed = false;
    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                   panic!("{}", e);
              }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   println!(
                       "client: {:?}, epoch: {}, step: {}, Loss: {}",
                       client, epoch, step, loss
                   );
                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}

#[ignore = "These tests are a bit flaky, so we need to make sure they work properly."]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_delay_solana_test_validator(
    #[values(1, 2)] n_clients: u8,
    #[values(0, 10)] delay_step: u64,
    #[values(1000, 5000)] delay_milis: i64,
) {
    // Test variables
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize docker watcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with n_clients clients
    let _cleanup = if n_clients == 1 {
        e2e_testing_setup(docker.clone(), 1, None).await
    } else {
        e2e_testing_setup(
            docker.clone(),
            2,
            Some(PathBuf::from(
                "../../config/solana-test/light-two-min-clients.toml",
            )),
        )
        .await
    };

    // Solana client
    let solana_client = SolanaTestClient::new(run_id).await;

    // Monitor clients
    for i in 1..=n_clients {
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i),
                vec![JsonFilter::Loss],
            )
            .unwrap();
    }

    // Sleep to let the coordinator to be deployed and run to be configured
    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = vec![format!("{VALIDATOR_CONTAINER_PREFIX}-1")];

    let chaos_scheduler = ChaosScheduler::new(docker.clone(), solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 120,
                latency_ms: delay_milis,
                targets: chaos_targets.clone(),
            },
            delay_step,
        )
        .await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
                   if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                       panic!("{}", e);
               }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   println!(
                       "client: {:?}, epoch: {}, step: {}, Loss: {}",
                       client, epoch, step, loss
                   );
                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}

#[ignore = "These tests are a bit flaky, so we need to make sure they work properly."]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_delay_solana_client(#[values(1, 2)] n_clients: u8, #[values(0, 10)] delay_step: u64) {
    // Test variables
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize docker watcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with n_clients clients
    let _cleanup = if n_clients == 1 {
        e2e_testing_setup(docker.clone(), 1, None).await
    } else {
        e2e_testing_setup(
            docker.clone(),
            2,
            Some(PathBuf::from(
                "../../config/solana-test/light-two-min-clients.toml",
            )),
        )
        .await
    };

    // Solana client
    let solana_client = SolanaTestClient::new(run_id).await;

    // Monitor clients
    for i in 1..=n_clients {
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i),
                vec![JsonFilter::Loss],
            )
            .unwrap();
    }

    // Sleep to let the coordinator to be deployed and run to be configured
    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = (1..=n_clients)
        .map(|i| format!("{CLIENT_CONTAINER_PREFIX}-{}", i))
        .collect::<Vec<String>>();

    let chaos_scheduler = ChaosScheduler::new(docker.clone(), solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 120,
                latency_ms: 1000,
                targets: chaos_targets.clone(),
            },
            delay_step,
        )
        .await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");
    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                   panic!("{}", e);
              }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   println!(
                       "client: {:?}, epoch: {}, step: {}, Loss: {}",
                       client, epoch, step, loss
                   );

                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}

/// Drop a client below the minimum required, go to WaitingForMembers
/// Reconnect a client and then go back to warmup
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn drop_a_client_waitingformembers_then_reconnect() {
    let n_clients = 2;
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    let _cleanup = e2e_testing_setup(
        docker.clone(),
        2,
        Some(PathBuf::from(
            "../../config/solana-test/light-two-min-clients.toml",
        )),
    )
    .await;
    let solana_client = SolanaTestClient::new(run_id).await;
    // Monitor clients
    for i in 1..=n_clients {
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i),
                vec![
                    JsonFilter::Loss,
                    JsonFilter::StateChange,
                    JsonFilter::LoadedModel,
                ],
            )
            .unwrap();
    }

    let mut warmup_reached = false;
    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(timestamp, client, old_state, new_state) => {
                let coordinator_state = solana_client.get_run_state().await;
                println!(
                    "state change client {} - {}=>{}",
                    client, new_state, old_state
                );

                // Once warmup starts, kill client 2's container
                if new_state == RunState::Warmup.to_string() && !warmup_reached {
                    println!(
                        "Warmup started, killing container {}...",
                        &format!("{CLIENT_CONTAINER_PREFIX}-2")
                    );

                    let options = Some(KillContainerOptions { signal: "SIGKILL" });
                    docker
                        .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-2"), options)
                        .await
                        .unwrap();

                    tokio::time::sleep(Duration::from_secs(2)).await;
                    warmup_reached = true;
                }

                // After killing client, verify we get stuck in WaitingForMembers
                if warmup_reached && coordinator_state == RunState::WaitingForMembers {
                    println!("WaitingForMembers seen");
                    break;
                }
            }
            Response::Loss(client, epoch, step, loss) => {
                println!(
                    "client: {:?}, epoch: {}, step: {}, Loss: {}",
                    client, epoch, step, loss
                );

                if epoch as i64 > current_epoch {
                    current_epoch = epoch as i64;
                    //assert!(loss < last_epoch_loss);
                    last_epoch_loss = loss;
                    if epoch == num_of_epochs_to_run {
                        println!("Epoch {} reached. Stopping", epoch);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    println!("Waiting 5s to see if we are still in WaitingForMembers...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let coordinator_state = solana_client.get_run_state().await;
    assert_eq!(coordinator_state, RunState::WaitingForMembers);
    println!("Still in WaitingForMembers after 5 seconds. Success");

    // Test reconnection
    println!("Starting new client...");
    spawn_new_client(docker.clone()).await.unwrap();

    // Wait for state to change back to Warmup
    assert!(
        solana_client.wait_for_run_state(RunState::Warmup, 30).await,
        "System should have returned to Warmup state after client reconnection"
    );
    println!("Successfully returned to Warmup state after client reconnection");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_when_all_clients_disconnect_checkpoint_is_hub() {
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    let _cleanup = e2e_testing_setup(
        docker.clone(),
        1,
        Some(PathBuf::from("../../config/solana-test/light-config.toml")),
    )
    .await;

    let _monitor_client = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-{}", 1),
            vec![
                JsonFilter::Loss,
                JsonFilter::StateChange,
                JsonFilter::LoadedModel,
            ],
        )
        .unwrap();

    let solana_client = SolanaTestClient::new(run_id).await;

    let mut has_killed_container_yet = false;
    let mut has_spawned_new_client_yet = false;
    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::Loss(client, epoch, step, loss) => {
                println!(
                    "client: {:?}, epoch: {}, step: {}, Loss: {}",
                    client, epoch, step, loss
                );
                if current_epoch == 1 && !has_killed_container_yet {
                    if !has_spawned_new_client_yet {
                        // Spawn a new client, that should get the model with P2P
                        spawn_new_client_with_monitoring(docker.clone(), &watcher)
                            .await
                            .unwrap();
                        has_spawned_new_client_yet = true;

                        // Checkpoint should be P2P here
                        assert!(
                            matches!(solana_client.get_checkpoint().await, Checkpoint::P2P(_)),
                            "The coordinator must be on P2P checkpoint"
                        );
                        println!("Checkpoint was P2P");
                        continue;
                    }
                    println!("Killing all clients to test checkpoint change to Hub");
                    kill_all_clients(&docker, "SIGKILL").await;
                    has_killed_container_yet = true;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    // Spawn a new client, that should get the model with Hub
                    spawn_new_client_with_monitoring(docker.clone(), &watcher)
                        .await
                        .unwrap();
                    println!("Spawned new client to test checkpoint change to Hub");
                }
                if epoch as i64 > current_epoch {
                    current_epoch = epoch as i64;
                    assert!(loss < last_epoch_loss);
                    last_epoch_loss = loss;
                    if epoch == num_of_epochs_to_run {
                        println!("Epoch {} reached. Stopping", epoch);
                        break;
                    }
                }
            }
            Response::LoadedModel(checkpoint) => {
                if !has_killed_container_yet {
                    continue;
                }
                assert!(
                    matches!(solana_client.get_checkpoint().await, Checkpoint::Hub(_)),
                    "The coordinator must be on Hub checkpoint"
                );
                println!("Client got the model from Hub");
                assert_eq!(
                    checkpoint, "emozilla/llama2-20m-init",
                    "The model should be obtained from Hub"
                );
                return;
            }
            _ => {}
        }
    }
}
