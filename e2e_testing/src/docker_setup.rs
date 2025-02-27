use bollard::container::{ListContainersOptions, RemoveContainerOptions};
use bollard::models::DeviceRequest;
use bollard::Docker;
use bollard::{
    container::{Config, CreateContainerOptions},
    secret::HostConfig,
};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::signal;

use crate::docker_watcher::DockerWatcherError;

pub const CLIENT_CONTAINER_PREFIX: &str = "test-psyche-test-client";

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

pub async fn spawn_new_client(docker_client: Arc<Docker>) -> Result<(), DockerWatcherError> {
    let all_containers = docker_client
        .list_containers::<String>(Some(ListContainersOptions {
            all: true, // Include stopped containers as well
            ..Default::default()
        }))
        .await
        .unwrap();

    let mut running_containers = Vec::new();
    let mut all_container_names = Vec::new();

    for cont in all_containers {
        if let Some(names) = &cont.names {
            if let Some(name) = names.first() {
                let trimmed_name = name.trim_start_matches('/').to_string();

                if trimmed_name.starts_with(CLIENT_CONTAINER_PREFIX) {
                    all_container_names.push(trimmed_name.clone());

                    if cont
                        .state
                        .as_deref()
                        .is_some_and(|state| state.eq_ignore_ascii_case("running"))
                    {
                        running_containers.push(trimmed_name);
                    }
                }
            }
        }
    }

    // Set the container name based on the ones that are already running.
    let new_container_name = format!("{CLIENT_CONTAINER_PREFIX}-{}", running_containers.len() + 1);
    // Check if container was already created.
    let container_exists = all_container_names.contains(&new_container_name);

    if container_exists {
        println!("Removing existing container: {}", new_container_name);
        docker_client
            .remove_container(
                &new_container_name,
                Some(RemoveContainerOptions {
                    force: true, // Ensure it's removed even if running
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
    }

    // Setting nvidia usage parameters
    let device_request = DeviceRequest {
        driver: Some("nvidia".to_string()),
        count: Some(1),
        capabilities: Some(vec![vec!["gpu".to_string()]]),
        ..Default::default()
    };

    // Setting extra hosts and nvidia request
    let network_name = "test_psyche-test-network";
    let host_config = HostConfig {
        device_requests: Some(vec![device_request]),
        extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        network_mode: Some(network_name.to_string()),
        ..Default::default()
    };

    // Get env vars from config file
    let env_vars = std::fs::read_to_string("../../../config/client/.env.local")
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();
    let envs = [env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat();

    let options = Some(CreateContainerOptions {
        name: new_container_name.clone(),
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
    // Start the container
    docker_client
        .start_container::<String>(&new_container_name, None)
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
