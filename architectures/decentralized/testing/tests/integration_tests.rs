use std::sync::Arc;

use bollard::Docker;
use e2e_testing::{
    docker_setup::e2e_testing_setup,
    docker_watcher::{DockerWatcher, JsonFilter, Response},
};
use psyche_coordinator::RunState;
use psyche_decentralized_testing::utils::SolanaTestClient;

/// spawn 1 client and run 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn one_client_three_epochs_run() {
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
            "test-psyche-test-client-1",
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(timestamp, _client_1, old_state, new_state, _epoch, _step) => {
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
            _ => panic!(),
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn disconnect_client() {
    // set test variables
    let run_id = "test".to_string();
    // epochs the test will run
    let num_of_epochs_to_run = 3;

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(2);

    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());
    let _monitor_client_1 = watcher
        .monitor_container(
            "test-psyche-test-client-1",
            vec![
                JsonFilter::StateChange,
                JsonFilter::Loss,
                JsonFilter::HealthCheck,
            ],
        )
        .unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    let mut client_1_id = "";
    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::JoinRun(client_id, run_id) => {
                client_1_id = &client_id;
            }
            Response::StateChange(timestamp, _client_1, old_state, new_state, epoch, step) => {
                let coordinator_state = solana_client.get_run_state().await;
                println!(
                    "client: new_state: {}, old_state: {}, timestamp: {}, epoch: {}, step: {}",
                    new_state, old_state, timestamp, epoch, step
                );

                // kill client 2 in step 3
                if epoch == 1 && step == 6 && new_state == RunState::RoundWitness.to_string() {
                    assert_eq!(solana_client.get_clients_len().await, 2);

                    watcher
                        .kill_container("test-psyche-test-client-2")
                        .await
                        .unwrap();
                    println!("Kill node test-psyche-test-client-2")
                }

                if step == 10 && new_state == RunState::RoundWitness.to_string() {
                    assert_eq!(solana_client.get_clients_len().await, 2);
                }

                if epoch == num_of_epochs_to_run {
                    break;
                }
            }

            Response::Loss(client_id, epoch, step, loss) => {
                println!(
                    "client_id: {:?}, epoch: {}, step: {}, Loss: {}",
                    client_id, epoch, step, loss
                );
            }
            Response::HealthCheck(unhealthy_client_id, _index) => {
                println!("found unhealthy client: {:?}", unhealthy_client_id);
                // let [_clients_1, client_2] = solana_client
                //     .get_clients()
                //     .await
                //     .to_vec()
                //     .map(|x| x.id)
                //     .try_into()
                //     .unwrap();

                // assert_eq!(unhealthy_client_id, client_2.id.to_string());
                // assert_eq!( unhealthy_client_id, client_2.id.to_string());
            }
        }
    }
    println!("Clients: {:?} ", solana_client.get_clients().await);

    // assert_with_retries(|| solana_client.get_run_state(), RunState::Cooldown).await;
}
