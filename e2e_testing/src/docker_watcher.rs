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
    State(StateFilter),
    StateChange,
}

impl JsonFilter {
    pub fn state_change() -> Self {
        Self::StateChange
    }

    pub fn state(state: StateFilter) -> Self {
        Self::State(state)
    }
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

// struct DockerWatcher<T> {
pub struct DockerWatcher {
    client: Arc<Docker>,
    log_sender: mpsc::Sender<String>,
    // channel: Sender<T>,
}

// impl<T> DockerWatcher<T> {
impl DockerWatcher {
    pub fn new(client: Arc<Docker>, log_sender: mpsc::Sender<String>) -> Self {
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
            println!("spawn task monitor_handle");
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

                println!("Logs: {:?}", &parsed_log);

                match filter {
                    JsonFilter::State(ref state) => {
                        let Some(parsed_new_state) =
                            parsed_log.get("new_state").and_then(|v| v.as_str())
                        else {
                            continue;
                        };
                        if state.compare_state(parsed_new_state) {
                            println!("NEW STATE: {}", parsed_new_state);
                        }
                    }
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

                            let message = format!(
                                "[CLIENT {client_id}] state change: {old_state} -> {new_state}"
                            );
                            println!("{:?}", message);
                            log_sender.send(message).await.unwrap()
                            // client.
                        }
                    }
                }
            }
            Ok(())
        });

        Ok(monitor_handle)
    }
}
