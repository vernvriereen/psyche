use bollard::container::{
    ListContainersOptions, RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::models::DeviceRequest;
use bollard::Docker;
use bollard::{
    container::{Config, CreateContainerOptions},
    secret::HostConfig,
};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::signal;

use crate::docker_watcher::DockerWatcherError;

pub const CLIENT_CONTAINER_PREFIX: &str = "test-psyche-test-client";
pub const VALIDATOR_CONTAINER_PREFIX: &str = "test-psyche-solana-test-validator";

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

/// FIXME: The config path must be relative to the compose file for now.
pub fn e2e_testing_setup(init_num_clients: usize, config: Option<PathBuf>) -> DockerTestCleanup {
    spawn_psyche_network(init_num_clients, config).unwrap();
    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub async fn is_client_healthy(
    docker_client: Arc<Docker>,
    client_number: u8,
) -> Result<bool, DockerWatcherError> {
    let container_name = format!("{CLIENT_CONTAINER_PREFIX}-{}", client_number);
    let container = docker_client
        .inspect_container(&container_name, None)
        .await
        .unwrap();
    let state = container.state.unwrap();
    match state.status {
        Some(bollard::secret::ContainerStateStatusEnum::DEAD)
        | Some(bollard::secret::ContainerStateStatusEnum::EXITED) => Ok(false),
        _ => Ok(true),
    }
}

pub async fn stop_solana_validator(
    docker_client: Arc<Docker>,
    after_secs: Option<i64>,
) -> Result<(), DockerWatcherError> {
    let options = after_secs.map(|time| StopContainerOptions { t: time });
    docker_client
        .stop_container(&format!("{VALIDATOR_CONTAINER_PREFIX}-1"), options)
        .await
        .unwrap();
    println!("Validator stopped");
    Ok(())
}

pub async fn restart_solana_validator(
    docker_client: Arc<Docker>,
) -> Result<(), DockerWatcherError> {
    docker_client
        .start_container::<String>(&format!("{VALIDATOR_CONTAINER_PREFIX}-1"), None)
        .await
        .unwrap();
    Ok(())
}

pub async fn add_delay(
    docker_client: Arc<Docker>,
    target: &[&str],
    duration_secs: u64,
    delay_milis: u64,
) -> Result<(), DockerWatcherError> {
    let container_name = "pumba-chaos";

    let network_name = "test_psyche-test-network";
    let host_config = HostConfig {
        network_mode: Some(network_name.to_string()),
        binds: Some(vec!["/var/run/docker.sock:/var/run/docker.sock".to_string()]),
        ..Default::default()
    };

    // Create the container with the Pumba image
    let create_options = CreateContainerOptions {
        name: container_name,
        ..Default::default()
    };

    let _ = docker_client
        .remove_container(
            container_name,
            Some(RemoveContainerOptions {
                force: true, // Ensure it's removed even if running
                ..Default::default()
            }),
        )
        .await;

    let duration = format!("{duration_secs}s");
    let delay_milis = format!("{delay_milis}");
    let mut entry_command = vec![
        "netem",
        "--duration",
        &duration,
        "delay",
        "--jitter",
        "500",
        "--time",
        &delay_milis,
    ];
    for target in target.iter() {
        entry_command.push(target);
    }
    let container = docker_client
        .create_container(
            Some(create_options),
            bollard::container::Config {
                image: Some("gaiaadm/pumba:latest"),
                cmd: Some(entry_command),
                host_config: Some(host_config),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Start the container
    docker_client
        .start_container(&container.id, None::<StartContainerOptions<&str>>)
        .await
        .unwrap();

    println!("Delay applied for containers: {:?}", target.to_vec());
    Ok(())
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
        image: Some("psyche-test-client"),
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

pub fn spawn_psyche_network(
    init_num_clients: usize,
    config: Option<PathBuf>,
) -> Result<(), DockerWatcherError> {
    let mut command = Command::new("just");
    let command = command
        .args(["setup_test_infra", &format!("{}", init_num_clients)])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(config) = config {
        command.env("CONFIG_PATH", config);
    }

    let output = command
        .output()
        .expect("Failed to spawn docker compose instances");
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
