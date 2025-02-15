use std::collections::HashMap;
use std::default::Default;
use std::sync::Arc;

use bollard::container::ListContainersOptions;
use bollard::Docker;
use e2e_testing::docker_watcher::{DockerWatcher, JsonFilter};
use futures_util::future::join_all;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Arc::new(Docker::connect_with_socket_defaults()?);
    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);

    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;

    for container in containers {
        println!("Container name: {:?}", container.names);
    }
    println!();

    // let filter = JsonFilter::State("RoundTrain".to_string());
    // let filter_2 = JsonFilter::State("RoundWitness".to_string());
    let state_change_filter = JsonFilter::state_change();

    let watcher = DockerWatcher::new(docker.clone());
    let handle_1 = watcher
        .monitor_container("psyche-psyche-test-client-1", state_change_filter)
        .unwrap();
    let handle_2 = watcher
        .monitor_container("psyche-psyche-test-client-2", state_change_filter)
        .unwrap();

    join_all(vec![handle_1, handle_2]).await;

    Ok(())
}
