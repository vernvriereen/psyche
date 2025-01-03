use std::{collections::HashMap, path::PathBuf};

use psyche_coordinator::{
    model::{self, HubRepo},
    Coordinator, SOLANA_MAX_STRING_LEN,
};
use psyche_core::NodeIdentity;
use psyche_data_provider::{upload_model_repo_async, UploadModelError};
use psyche_modeling::{save_tensors_into_safetensors, SaveSafetensorsError};
use tch::Tensor;
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{info, info_span, Instrument};

use crate::{
    trainer::{Trainer, TrainerThreadCommunicationError},
    HubUploadInfo,
};

use super::{
    evals::{EvalRunner, RunningEvals},
    CheckpointConfig,
};

#[derive(Error, Debug)]
pub enum CooldownError {
    #[error("no trainers available for checkpointing!")]
    NoTrainers,

    #[error("checkpointing thread crashed")]
    CheckpointThreadCrashed,

    #[error("error while checkpointing: {0}")]
    Checkpoint(#[from] CheckpointError),
}

pub struct CooldownStepMetadata {
    tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
    tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
    checkpoint_info: Option<CheckpointConfig>,
    checkpoint_extra_files: Vec<PathBuf>,

    eval_runner: EvalRunner,
}

impl CooldownStepMetadata {
    pub fn new(
        tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
        tx_model: mpsc::UnboundedSender<HashMap<String, Tensor>>,
        checkpoint_info: Option<CheckpointConfig>,
        checkpoint_extra_files: Vec<PathBuf>,
        eval_runner: EvalRunner,
    ) -> Self {
        Self {
            tx_checkpoint,
            tx_model,
            checkpoint_info,
            checkpoint_extra_files,
            eval_runner,
        }
    }
}

#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Extract thread crashed")]
    ExtractThreadCrashed,

    #[error("Trainer extract error: {0}")]
    Extract(#[from] TrainerThreadCommunicationError),

    #[error("Write thread crashed")]
    WriteThreadCrashed,

    #[error("Writing safetensors to disk failed: {0}")]
    WriteSafetensors(#[from] SaveSafetensorsError),

    #[error("Writing extra file to disk failed: {0}")]
    WriteExtraFile(#[from] tokio::io::Error),

    #[error("Couldn't upload model to huggingface: {0}")]
    UploadError(#[from] UploadModelError),

    #[error("Couldn't send checkpoint - channel closed")]
    SendCheckpoint,
}

impl CooldownStepMetadata {
    pub fn start<T: NodeIdentity>(
        &self,
        mut trainers: Vec<Trainer>,
        state: &Coordinator<T>,
    ) -> Result<CooldownStep, CooldownError> {
        let Some(mut trainer) = trainers.pop() else {
            return Err(CooldownError::NoTrainers);
        };

        let step = state.progress.step - 1;
        let run_id = String::from_utf8(state.run_id.clone().to_vec()).unwrap();
        let checkpoint_extra_files = self.checkpoint_extra_files.clone();
        let checkpoint_info = self.checkpoint_info.clone();
        let tx_checkpoint = self.tx_checkpoint.clone();
        let tx_model = self.tx_model.clone();
        let eval_runner = self.eval_runner.clone();

        let checkpointing_and_evals = tokio::task::spawn(
            async move {
                info!("Extracting full model");
                let (variables, trainer) =
                    tokio::task::spawn_blocking::<_, Result<_, CheckpointError>>(|| {
                        let variables = trainer.extract()?;
                        Ok((variables, trainer))
                    })
                    .await
                    .map_err(|_| CheckpointError::ExtractThreadCrashed)??;

                trainers.push(trainer);
                let evals = eval_runner.start(trainers);

                let Some(CheckpointConfig {
                    hub_upload,
                    checkpoint_dir,
                }) = checkpoint_info
                else {
                    // FIXME(marian): Here we are assuming that we either we upload the model to HF or
                    // we share it by p2p, but in principle both could be possible.
                    tx_model
                        .send(variables)
                        .map_err(|_| CheckpointError::SendCheckpoint)?;

                    return Ok(evals);
                };

                let path = checkpoint_dir.join(format!("{run_id}-step{step}"));
                info!("Saving to {}", path.display());

                let mut local = tokio::task::spawn_blocking({
                    let path = path.clone();
                    move || save_tensors_into_safetensors(variables, path)
                })
                .await
                .map_err(|_| CheckpointError::WriteThreadCrashed)??;

                for extra in checkpoint_extra_files {
                    let to = path.join(extra.file_name().unwrap());
                    tokio::fs::copy(extra.clone(), to.clone())
                        .await
                        .map_err(CheckpointError::WriteExtraFile)?;
                    local.push(to);
                }

                let Some(HubUploadInfo {
                    hub_repo,
                    hub_token,
                }) = hub_upload
                else {
                    return Ok(evals);
                };

                info!("Uploading to {}", hub_repo);
                let mut hub_repo_bytes = [0u8; SOLANA_MAX_STRING_LEN]; // Initialize an array with zeros
                let bytes = hub_repo.as_bytes(); // Convert the string to bytes
                let len = bytes.len().min(SOLANA_MAX_STRING_LEN); // Limit to 64 bytes if the input is too long
                hub_repo_bytes[..len].copy_from_slice(&bytes[..len]); // Copy the bytes into the array
                let revision = upload_model_repo_async(
                    hub_repo.clone(),
                    local,
                    hub_token.clone(),
                    Some(format!("step {step}")),
                    None,
                )
                .await?;
                let mut revision_bytes = [0u8; SOLANA_MAX_STRING_LEN]; // Initialize an array with zeros
                let bytes = revision.as_bytes(); // Convert the string to bytes
                let len = bytes.len().min(SOLANA_MAX_STRING_LEN); // Limit to 64 bytes if the input is too long
                revision_bytes[..len].copy_from_slice(&bytes[..len]); // Copy the bytes into the array

                tx_checkpoint
                    .send(model::Checkpoint::Hub(HubRepo {
                        repo_id: hub_repo_bytes,
                        revision: Some(revision_bytes),
                    }))
                    .map_err(|_| CheckpointError::SendCheckpoint)?;

                Ok(evals)
            }
            .instrument(info_span!("checkpointing")),
        );
        Ok(CooldownStep {
            checkpointing_and_evals,
        })
    }
}

#[derive(Debug)]
pub struct CooldownStep {
    checkpointing_and_evals: JoinHandle<Result<RunningEvals, CheckpointError>>,
}

impl CooldownStep {
    pub async fn finish(self) -> Result<RunningEvals, CooldownError> {
        let running_evals = self
            .checkpointing_and_evals
            .await
            .map_err(|_| CooldownError::CheckpointThreadCrashed)??;

        Ok(running_evals)
    }
}
