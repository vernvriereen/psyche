use std::{collections::HashMap, sync::Arc, time::Duration};

use bollard::Docker;
use e2e_testing::{
    docker_setup::e2e_testing_setup,
    docker_watcher::{DockerWatcher, JsonFilter, Response},
};
use psyche_decentralized_testing::utils::SolanaTestClient;
use tokio::sync::mpsc;

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn get_state() {
    let run_id = String::from("test");
    let client = SolanaTestClient::new(run_id).await;
    println!("state: {:?}", client.get_run_state().await);
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn happy_path() {
    let run_id = "test".to_string();
    let _cleanup = e2e_testing_setup(2);

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let state_change_filter = JsonFilter::StateChange;
    let loss_filter = JsonFilter::Loss;

    let (tx, mut rx) = mpsc::channel(100);
    let watcher = DockerWatcher::new(docker.clone(), tx);
    let handle_1 = watcher
        .monitor_container("test-psyche-test-client-1", state_change_filter)
        .unwrap();
    let handle_2 = watcher
        .monitor_container("test-psyche-test-client-1", loss_filter)
        .unwrap();
    let handle_3 = watcher
        .monitor_container("test-psyche-test-client-2", state_change_filter)
        .unwrap();

    let solana_network = SolanaTestClient::new(run_id).await;
    // println!("state: {:?}", solana_network.get_run_state().await);

    // let client_1 = solana_network.get_clients().await[0].id.to_string();
    // let client_2 = solana_network.get_clients().await[1].id.to_string();
    while solana_network.get_clients_len().await < 2 {
        println!("Waiting for members, actual: {}", solana_network.get_clients_len().await);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    while let Some(response) = rx.recv().await {
        match response {
            Response::StateChange(client_1, old_state, new_state) => {
                let coordinator_state = solana_network.get_run_state().await;
                // assert_eq!(new_state, coordinator_state.to_string());
                println!("NEW STATE: {}", new_state);
                println!("COORDINATOR STATE: {}", coordinator_state);
            }
            Response::StateChange(client_2, old_state, new_state) => {
                let coordinator_state = solana_network.get_run_state().await;
                println!("NEW STATE: {}", new_state);
                println!("COORDINATOR STATE: {}", coordinator_state);
                // assert_eq!(new_state, coordinator_state.to_string());
            }
            // let response = Response::Loss(client_id, epoch, step, loss);
            Response::Loss(client_1, epoch, step, loss) => {
                println!("Client: {:?}, Loss: {}", client_1, loss);
            }
            Response::Loss(client_2, epoch, step, loss) => {
                println!("Client: {:?}, Loss: {}", client_2, loss);
            }
        }
        // println!("{:?}", response)
    }
}
