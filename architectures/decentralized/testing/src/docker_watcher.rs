use std::time::SystemTime;
use std::{sync::Arc, time::Duration};

use crate::CLIENT_CONTAINER_PREFIX;
use bollard::{container::LogsOptions, Docker};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone, Copy)]
pub enum StateFilter {
    Warmup,
    RoundTrain,
    RoundWitness,
}

#[derive(Clone, Copy)]
pub enum JsonFilter {
    StateChange,
    Loss,
    LoadedModel,
    HealthCheck,
}

#[derive(Debug)]
pub enum Response {
    StateChange(String, String, String, String, u64, u64),
    Loss(String, u64, u64, f64),
    LoadedModel(String),
    HealthCheck(String, u64, u64),
}

#[derive(thiserror::Error, Debug)]
pub enum DockerWatcherError {
    #[error("could not get current unix timestamp")]
    UnixTimestampError,

    #[error("could not convert duration {:?} into seconds", duration)]
    IntoSecondsError { duration: Duration },

    #[error("logging error: {inner}")]
    LogsError { inner: bollard::errors::Error },

    #[error("Client {0} has crashed")]
    ClientCrashedError(u8),
}

pub struct DockerWatcher {
    client: Arc<Docker>,
    log_tx: mpsc::Sender<Response>,
    pub log_rx: mpsc::Receiver<Response>,
}

impl DockerWatcher {
    pub fn new(client: Arc<Docker>) -> Self {
        let (log_tx, log_rx) = mpsc::channel(100);

        Self {
            client,
            log_tx,
            log_rx,
        }
    }

    pub fn monitor_container(
        &self,
        name: &str,
        filters: Vec<JsonFilter>,
    ) -> Result<JoinHandle<Result<(), DockerWatcherError>>, DockerWatcherError> {
        let Ok(duration) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
            return Err(DockerWatcherError::UnixTimestampError);
        };
        let Ok(current_unix_timestamp): Result<i64, _> = duration.as_secs().try_into() else {
            return Err(DockerWatcherError::IntoSecondsError { duration });
        };

        let log_options = Some(LogsOptions::<String> {
            stderr: true,
            stdout: true,
            follow: true,
            since: current_unix_timestamp,
            ..Default::default()
        });

        let name = name.to_string();
        let client = self.client.clone();
        let log_sender = self.log_tx.clone();
        let monitor_handle = tokio::spawn(async move {
            let mut logs = client.logs(&name, log_options);
            while let Some(log) = logs.next().await {
                let log = match log {
                    Ok(log) => log,
                    Err(e) => return Err(DockerWatcherError::LogsError { inner: e }),
                };
                let Ok(parsed_log): Result<Value, _> =
                    serde_json::from_slice(&log.clone().into_bytes())
                else {
                    continue;
                };

                for filter in &filters {
                    match filter {
                        JsonFilter::StateChange => {
                            let Some(old_state) =
                                parsed_log.get("old_state").and_then(|v| v.as_str())
                            else {
                                continue;
                            };

                            // unwrapping here, it should not be possible for a log to have new_state
                            // but no old_state
                            let new_state = parsed_log
                                .get("new_state")
                                .and_then(|v| v.as_str())
                                .unwrap();

                            if old_state != new_state {
                                let client_id = parsed_log
                                    .get("client_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap();

                                let timestamp = parsed_log
                                    .get("timestamp")
                                    .and_then(|v| v.as_str())
                                    .unwrap();
                                let epoch =
                                    parsed_log.get("epoch").and_then(|v| v.as_u64()).unwrap();
                                let step = parsed_log.get("step").and_then(|v| v.as_u64()).unwrap();

                                let response = Response::StateChange(
                                    timestamp.to_string(),
                                    client_id.to_string(),
                                    old_state.to_string(),
                                    new_state.to_string(),
                                    epoch,
                                    step,
                                );

                                if log_sender.send(response).await.is_err() {
                                    println!("Probably the test ended so we drop the log sender");
                                }
                            }
                        }
                        JsonFilter::Loss => {
                            let Some(loss) = parsed_log.get("loss").and_then(|v| v.as_f64()) else {
                                continue;
                            };
                            let client_id = parsed_log
                                .get("client_id")
                                .and_then(|v| v.as_str())
                                .unwrap()
                                .to_string();
                            let epoch = parsed_log.get("epoch").and_then(|v| v.as_u64()).unwrap();
                            let step = parsed_log.get("step").and_then(|v| v.as_u64()).unwrap();
                            let response = Response::Loss(client_id, epoch, step, loss);
                            if log_sender.send(response).await.is_err() {
                                println!("Probably the test ended so we drop the log sender");
                            }
                        }
                        JsonFilter::HealthCheck => {
                            let Some(_) = parsed_log.get("unhealthy_warn") else {
                                continue;
                            };
                            let client_id = parsed_log
                                .get("client_id")
                                .and_then(|v| v.as_str())
                                .unwrap()
                                .to_string();
                            let index = parsed_log.get("index").and_then(|v| v.as_u64()).unwrap();
                            let current_step = parsed_log
                                .get("current_step")
                                .and_then(|v| v.as_u64())
                                .unwrap();
                            let response = Response::HealthCheck(client_id, index, current_step);
                            log_sender.send(response).await.unwrap()
                        }
                        JsonFilter::LoadedModel => {
                            let Some(checkpoint) = parsed_log.get("checkpoint") else {
                                continue;
                            };
                            let checkpoint = serde_json::from_value(checkpoint.clone()).unwrap();
                            let response = Response::LoadedModel(checkpoint);
                            if log_sender.send(response).await.is_err() {
                                println!("Probably the test ended so we drop the log sender");
                            }
                        }
                    }
                }
            }
            Ok(())
        });

        Ok(monitor_handle)
    }

    pub async fn kill_container(&self, name: &str) -> Result<(), DockerWatcherError> {
        use bollard::container::KillContainerOptions;
        self.client
            .kill_container(name, Some(KillContainerOptions { signal: "SIGKILL" }))
            .await
            .map_err(|err| DockerWatcherError::LogsError { inner: err })
    }

    pub async fn monitor_clients_health(&self, num_clients: u8) -> Result<(), DockerWatcherError> {
        for i in 1..=num_clients {
            let container_name = format!("{CLIENT_CONTAINER_PREFIX}-{}", i);
            let container = self
                .client
                .inspect_container(&container_name, None)
                .await
                .unwrap();
            let state = container.state.unwrap();
            match state.status {
                Some(bollard::secret::ContainerStateStatusEnum::DEAD)
                | Some(bollard::secret::ContainerStateStatusEnum::EXITED) => {
                    return Err(DockerWatcherError::ClientCrashedError(i))
                }
                _ => continue,
            }
        }
        Ok(())
    }
}
