use bollard::models::DeviceRequest;
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions},
    secret::{ContainerSummary, HostConfig},
};
use std::{
    collections::HashMap,
    process::{Command, Stdio},
};
use tokio::signal;

use crate::docker_watcher::DockerWatcherError;

pub struct DockerTestCleanup;
impl Drop for DockerTestCleanup {
    fn drop(&mut self) {
        println!("\nStopping containers...");
        let output = Command::new("just")
            .args(["stop_test_infra"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

pub fn e2e_testing_setup(init_num_clients: usize) -> DockerTestCleanup {
    spawn_psyche_network(init_num_clients).unwrap();
    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub async fn spawn_new_client() -> Result<(), DockerWatcherError> {
    let docker_client = bollard::Docker::connect_with_socket_defaults().unwrap();
    let containers: Vec<ContainerSummary> =
        docker_client.list_containers::<String>(None).await.unwrap();
    let container_names: Vec<String> = containers
        .into_iter()
        .filter_map(|cont| {
            cont.names
                .clone()
                .unwrap_or_default()
                .get(0)
                .filter(|name| name.starts_with("/test-psyche-test-client"))
                .cloned()
        })
        .collect();

    // Define GPU capabilities
    let device_request = DeviceRequest {
        driver: Some("nvidia".to_string()),
        count: Some(1),
        capabilities: Some(vec![vec!["gpu".to_string()]]),
        ..Default::default()
    };

    let network_name = "test_psyche-test-network"; // Replace with your actual network
                                                   // Define host configuration
    let host_config = HostConfig {
        device_requests: Some(vec![device_request]),
        extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        network_mode: Some(network_name.to_string()), // Attach container to the network
        ..Default::default()
    };

    // Read environment variables from `.env` file
    let a = std::env::current_dir();
    println!("{}", a.unwrap().display());
    let env_vars = std::fs::read_to_string("../../../config/client/.env.local")
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    let envs = vec![env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat();

    let options = Some(CreateContainerOptions {
        name: format!("test-psyche-test-client-{}", container_names.len() + 1),
        platform: None,
    });
    let config = Config {
        image: Some("psyche-client"),
        env: Some(envs.iter().map(|s| s.as_str()).collect()),
        host_config: Some(host_config),
        ..Default::default()
    };
    docker_client
        .create_container(options, config)
        .await
        .unwrap();
    docker_client
        .start_container::<String>(
            &format!("test-psyche-test-client-{}", container_names.len() + 1),
            None,
        )
        .await
        .unwrap();
    Ok(())
}

pub fn spawn_psyche_network(init_num_clients: usize) -> Result<(), DockerWatcherError> {
    let output = Command::new("just")
        .args(["setup_test_infra", &format!("{}", init_num_clients)])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Failed spawn docker compose command");

    if !output.status.success() {
        panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\n[+] Docker compose network spawned successfully!");
    println!();

    Ok(())
}

pub fn spawn_ctrl_c_task() {
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        println!("\nCtrl+C received. Stopping containers...");
        let output = Command::new("just")
            .args(["stop_test_infra"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
        std::process::exit(0);
    });
}
