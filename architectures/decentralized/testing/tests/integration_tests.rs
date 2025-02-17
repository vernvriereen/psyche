use std::sync::Arc;

use bollard::Docker;
use e2e_testing::{
    docker_setup::e2e_testing_setup,
    docker_watcher::{DockerWatcher, JsonFilter},
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
    let _cleanup = e2e_testing_setup(2);

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let state_change_filter = JsonFilter::state_change();

    let (tx, mut rx) = mpsc::channel(100);
    let watcher = DockerWatcher::new(docker.clone(), tx);
    let handle_1 = watcher
        .monitor_container("psyche-psyche-test-client-1", state_change_filter)
        .unwrap();
    let handle_2 = watcher
        .monitor_container("psyche-psyche-test-client-2", state_change_filter)
        .unwrap();

    while let Some(message) = rx.recv().await {
        println!("{:?}", message)
    }
}
