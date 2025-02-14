use std::sync::Arc;

use bollard::{container::LogsOptions, Docker};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::task::JoinHandle;

pub enum JsonFilter {
    StateFilter(String),
}

// struct DockerWatcher<T> {
pub struct DockerWatcher {
    client: Arc<Docker>,
    // channel: Sender<T>,
}

// impl<T> DockerWatcher<T> {
impl DockerWatcher {
    pub fn new(client: Arc<Docker>) -> Self {
        Self { client }
    }

    pub fn monitor_container(&self, name: &str, filter: JsonFilter) -> JoinHandle<()> {
        let log_options = Some(LogsOptions::<String> {
            stderr: true,
            stdout: true,
            follow: true,
            ..Default::default()
        });

        let name = name.to_string();
        let client = self.client.clone();
        tokio::spawn(async move {
            let mut logs = client.logs(&name, log_options);
            while let Some(Ok(log)) = logs.next().await {
                let Ok(parsed): Result<Value, _> =
                    serde_json::from_slice(&log.clone().into_bytes())
                else {
                    continue;
                };
                match filter {
                    JsonFilter::StateFilter(ref state) => {
                        if parsed.get("new_state").and_then(|v| v.as_str()) == Some(&state) {
                            println!("NEW STATE: {}", state);
                        }
                    }
                }
            }
        })
    }
}
