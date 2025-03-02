use std::{collections::HashMap, sync::Arc};

use bollard::{
    container::{CreateContainerOptions, RemoveContainerOptions, StartContainerOptions},
    image::{CreateImageOptions, ListImagesOptions},
    secret::HostConfig,
    Docker,
};
use futures_util::StreamExt;

pub enum ChaosAction {
    Pause(i64),
    Delay(i64, i64),
    Kill,
}

pub async fn execute_chaos_action(
    docker_client: Arc<Docker>,
    action: ChaosAction,
    targets: Vec<String>,
) {
    let mut command: Vec<String> = match action {
        ChaosAction::Pause(duration_secs) => {
            let duration = format!("{duration_secs}s");
            vec!["pause".to_string(), "--duration".to_string(), duration]
        }
        ChaosAction::Delay(duration_secs, delay_milis) => {
            let duration = format!("{duration_secs}s");
            let delay_milis = format!("{delay_milis}");
            vec![
                "netem".to_string(),
                "--duration".to_string(),
                duration,
                "delay".to_string(),
                "--jitter".to_string(),
                "500".to_string(),
                "--time".to_string(),
                delay_milis,
            ]
        }
        ChaosAction::Kill => {
            vec!["kill".to_string()]
        }
    };

    pull_image(docker_client.clone()).await;
    create_chaos_action_with_comand(docker_client, targets.clone(), &mut command).await;
    println!("Chaos correctly applied for containers: {:?}", targets);
}

async fn pull_image(docker_client: Arc<Docker>) {
    let filters = HashMap::from([(
        "reference".to_string(),
        vec![format!("gaiaadm/pumba:latest")],
    )]);

    let options = ListImagesOptions {
        all: false,
        filters,
        ..Default::default()
    };

    if docker_client
        .list_images(Some(options))
        .await
        .unwrap()
        .is_empty()
    {
        println!("Pumba image not found, pulling from registry");
        let create_image_options = CreateImageOptions {
            from_image: "gaiaadm/pumba",
            tag: "latest",
            ..Default::default()
        };

        // Pull the image
        let mut stream = docker_client.create_image(Some(create_image_options), None, None);

        while stream.next().await.is_some() {}
        println!("Image pulled successfully!")
    } else {
        println!("Pumba image found in local registry");
    }
}

async fn create_chaos_action_with_comand(
    docker_client: Arc<Docker>,
    targets: Vec<String>,
    command: &mut Vec<String>,
) {
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

    for target in targets.iter() {
        command.push(target.clone());
    }

    let container = docker_client
        .create_container(
            Some(create_options),
            bollard::container::Config {
                image: Some("gaiaadm/pumba:latest"),
                cmd: Some(command.iter().map(|c| c.as_str()).collect()),
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
}
