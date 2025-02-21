use std::sync::Arc;

use bollard::Docker;
use e2e_testing::{
    docker_setup::e2e_testing_setup,
    docker_watcher::{DockerWatcher, JsonFilter, Response},
};
use psyche_coordinator::RunState;
use psyche_decentralized_testing::utils::SolanaTestClient;
use tokio::sync::mpsc;

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn happy_path() {
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
    let (tx, mut rx) = mpsc::channel(100);
    let watcher = DockerWatcher::new(docker.clone(), tx);
    let _monitor_client_1 = watcher
        .monitor_container(
            "test-psyche-test-client-1",
            vec![JsonFilter::StateChange, JsonFilter::Loss],
        )
        .unwrap();

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id).await;

    while let Some(response) = rx.recv().await {
        match response {
            Response::StateChange(timestamp, _client_1, old_state, new_state) => {
                let coordinator_state = solana_client.get_run_state().await;
                println!(
                    "client: new_state: {}, old_state: {}, timestamp: {}",
                    new_state, old_state, timestamp
                );
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
        }
    }
}
