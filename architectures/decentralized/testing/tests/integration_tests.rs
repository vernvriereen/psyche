use std::{path::PathBuf, sync::Arc, time::Duration};

use bollard::Docker;
use psyche_coordinator::{model::Checkpoint, RunState};
use psyche_decentralized_testing::{
    chaos::{ChaosAction, ChaosScheduler},
    docker_setup::{e2e_testing_setup, spawn_new_client},
    docker_watcher::{DockerWatcher, JsonFilter, Response},
    utils::SolanaTestClient,
    CLIENT_CONTAINER_PREFIX, VALIDATOR_CONTAINER_PREFIX,
};
use rstest::*;
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

    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1, None).await;

    // Monitor the client container
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    // Initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;
    let mut live_interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health(1).await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state, _ , _)) => {
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
                if let Err(e) = watcher.monitor_clients_health(n_new_clients + 1).await {
                    panic!("{}", e);
               }
               let current_epoch = solana_client.get_current_epoch().await;
               let current_step = solana_client.get_last_step().await;
               if current_epoch >= 1 && current_step > 1 {
                    panic!("Second epoch started and the clients did not get the model");
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
    let solana_client = Arc::new(SolanaTestClient::new(run_id).await);

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
    let solana_client = Arc::new(SolanaTestClient::new(run_id).await);

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
    let solana_client = Arc::new(SolanaTestClient::new(run_id).await);

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

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_rejoining_client_delay() {
    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1, None).await;

    let solana_client = Arc::new(SolanaTestClient::new("test".to_string()).await);

    tokio::time::sleep(Duration::from_secs(30)).await;

    // Spawn client
    spawn_new_client(docker.clone()).await.unwrap();

    let _monitor_client = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-{}", 2),
            vec![JsonFilter::LoadedModel],
        )
        .unwrap();

    let scheduler = ChaosScheduler::new(docker.clone(), solana_client.clone());
    scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 30,
                latency_ms: 3000,
                targets: vec![format!("{CLIENT_CONTAINER_PREFIX}-{}", 1)],
            },
            20,
        )
        .await;

    let mut interval = time::interval(Duration::from_secs(10));
    println!("Waiting for training to start");
    loop {
        tokio::select! {
           _ = interval.tick() => {
               println!("Waiting for first epoch to finish");
               if let Err(e) = watcher.monitor_clients_health(2).await {
                   panic!("{}", e);
               }
               let current_epoch = solana_client.get_current_epoch().await;
               let current_step = solana_client.get_last_step().await;
               if current_epoch >= 1 && current_step > 1 {
                    panic!("Second epoch started and the clients did not get the model");
               }
           }
           response = watcher.log_rx.recv() => {
               match response {
                   Some(Response::LoadedModel(checkpoint)) => {
                       // assert client and coordinator state synchronization
                       assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                       println!("Client got the model with P2P");
                       return;
                   }
                   _ => {}
               }
           }
        }
    }
}
/// creates a run and spawns 2 clients
/// the first run is completed only by the client that joins first
/// the second run starts with both clients and shuts down client 2 mid-run
/// verifies the Healthcheck is sent and the shutdown client is kicked from the run
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn disconnect_client() {
    // set test variables
    let run_id = "test".to_string();
    // epochs the test will run
    let num_of_epochs_to_run = 3;

    // initialize a Solana run with 2 client
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());
    let _cleanup = e2e_testing_setup(docker.clone(), 2, None).await;

    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![
                JsonFilter::StateChange,
                JsonFilter::HealthCheck,
                JsonFilter::UntrainedBatches,
            ],
        )
        .unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    // step where the healhcheck are send
    let mut health_check_step: Vec<u64> = Vec::new();
    // untrained batches
    let mut untrained_batches: Vec<Vec<u64>> = Vec::new();

    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(_timestamp, _client_id, old_state, new_state, epoch, step) => {
                let epoch_clients = solana_client.get_current_epoch_clients().await;

                if old_state == RunState::WaitingForMembers.to_string() {
                    println!(
                        "Starting epoch: {} with {} clients",
                        epoch,
                        epoch_clients.len()
                    );
                }

                // kill client during step 2 in the RoundWitness state
                if epoch == 1 && step == 22 && new_state == RunState::RoundWitness.to_string() {
                    assert_eq!(epoch_clients.len(), 2);

                    // kill client container
                    // take into account that it can take some time to the container to shutdown
                    // so the client can continue training for some extra steps
                    watcher
                        .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-2"))
                        .await
                        .unwrap();
                    println!("STOP NODE: {}-2", CLIENT_CONTAINER_PREFIX);
                }

                // one step after the healthcheck was send
                // the unhealthy client should be removed
                if !health_check_step.is_empty()
                    && health_check_step[0] + 1 == step
                    && new_state == RunState::RoundTrain.to_string()
                {
                    // assert idle client was kicked
                    assert_eq!(epoch_clients.len(), 1);
                }

                // finish test
                if epoch == num_of_epochs_to_run {
                    break;
                }
            }

            // track HealthChecks send
            Response::HealthCheck(unhealthy_client_id, _index, current_step) => {
                println!("found unhealthy client: {:?}", unhealthy_client_id);

                let clients_ids: Vec<String> = solana_client
                    .get_clients()
                    .await
                    .iter()
                    .map(|client| client.id.to_string())
                    .collect();
                health_check_step.push(current_step);
                assert!(clients_ids.contains(&unhealthy_client_id));
            }

            // track untrained batches
            Response::UntrainedBatches(untrained_batch_ids) => {
                println!("untrained_batch_ids: {:?}", untrained_batch_ids);
                untrained_batches.push(untrained_batch_ids);
            }
            _ => {}
        }
    }

    // assert that just one healthcheck was send
    assert!(health_check_step.len() == 1);

    // check how many batches where lost due to the client shutdown
    // ideally, we should only lose 2 batches (The ones assigned in the step where it didn't train and the ones where it ran the Health Check and gets kicked)
    // see issue: https://github.com/NousResearch/psyche/issues/269
    assert!(untrained_batches.len() <= 3);
}
