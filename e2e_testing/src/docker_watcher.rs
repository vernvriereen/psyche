use std::time::SystemTime;
use std::{sync::Arc, time::Duration};

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

impl StateFilter {
    fn compare_state(&self, state: &str) -> bool {
        match self {
            Self::Warmup => "Warmup" == state,
            Self::RoundTrain => "RoundTrain" == state,
            Self::RoundWitness => "RoundWitness" == state,
        }
    }
}

#[derive(Clone, Copy)]
pub enum JsonFilter {
    StateChange,
    Loss,
}

#[derive(thiserror::Error, Debug)]
pub enum DockerWatcherError {
    #[error("could not get current unix timestamp")]
    UnixTimestampError,

    #[error("could not convert duration {:?} into seconds", duration)]
    IntoSecondsError { duration: Duration },

    #[error("logging error: {inner}")]
    LogsError { inner: bollard::errors::Error },
}

pub struct DockerWatcher {
    client: Arc<Docker>,
    log_sender: mpsc::Sender<Response>,
}

#[derive(Debug)]
pub enum Response {
    StateChange(String, String, String),
    Loss(String, u64, u64, f64),
}

// impl<T> DockerWatcher<T> {
impl DockerWatcher {
    pub fn new(client: Arc<Docker>, log_sender: mpsc::Sender<Response>) -> Self {
        Self { client, log_sender }
    }

    pub fn monitor_container(
        &self,
        name: &str,
        filter: JsonFilter,
    ) -> Result<JoinHandle<Result<(), DockerWatcherError>>, DockerWatcherError> {
        println!("EMPEZO monitor_container");
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
        let log_sender = self.log_sender.clone();
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

                match filter {
                    JsonFilter::StateChange => {
                        let Some(old_state) = parsed_log.get("old_state").and_then(|v| v.as_str())
                        else {
                            continue;
                        };

                        // unwraping here, it should not be possible for a log to have new_state
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

                            let response = Response::StateChange(
                                client_id.to_string(),
                                old_state.to_string(),
                                new_state.to_string(),
                            );
                            log_sender.send(response).await.unwrap()
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
                        log_sender.send(response).await.unwrap()
                    }
                }
            }
            Ok(())
        });

        Ok(monitor_handle)
    }
}
