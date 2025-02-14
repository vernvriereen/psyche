use std::collections::HashMap;
use std::default::Default;
use std::sync::Arc;

use bollard::container::ListContainersOptions;
use bollard::Docker;
use e2e_testing::docker_watcher::{DockerWatcher, JsonFilter};
use futures_util::future::join_all;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
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

    let filter = JsonFilter::StateFilter("RoundTrain".to_string());
    let filter_2 = JsonFilter::StateFilter("RoundWitness".to_string());
    let watcher = DockerWatcher::new(docker.clone());
    let handle_1 = watcher.monitor_container("psyche-psyche-test-client-1", filter);
    let handle_2 = watcher.monitor_container("psyche-psyche-test-client-2", filter_2);

    join_all(vec![handle_1, handle_2]).await;

    Ok(())
}
