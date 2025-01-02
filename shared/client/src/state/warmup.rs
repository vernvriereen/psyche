use crate::trainer::Trainer;

use super::evals::{EvalRunner, RunningEvals};
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::info;

#[derive(Error, Debug)]
pub enum WarmupError {
    #[error("no trainers available for p2p model sharing!")]
    NoTrainers,
    #[error("extract thread crashed")]
    ExtractThreadCrashed,
}

pub struct WarmupStepMetadata {
    pub eval_runner: EvalRunner,
}

impl WarmupStepMetadata {
    pub fn start(&self, trainers: Vec<Trainer>) -> Result<WarmupStep, WarmupError> {
        let mut trainers = trainers;
        let Some(mut trainer) = trainers.pop() else {
            return Err(WarmupError::NoTrainers);
        };

        let eval_runner = self.eval_runner.clone();
        let evals = tokio::task::spawn(async move {
            info!("Extracting full model for p2p sharing");
            let (_variables, trainer) =
                tokio::task::spawn_blocking::<_, Result<_, WarmupError>>(move || {
                    let variables = trainer
                        .extract()
                        .map_err(|_| WarmupError::ExtractThreadCrashed)?;

                    Ok((variables, trainer))
                })
                .await
                .map_err(|_| WarmupError::ExtractThreadCrashed)??;

            trainers.push(trainer);

            let evals = eval_runner.start(trainers);
            Ok(evals)
        });

        Ok(WarmupStep { evals })
    }
}

#[derive(Debug)]
pub struct WarmupStep {
    evals: JoinHandle<Result<RunningEvals, WarmupError>>,
}

impl WarmupStep {
    pub async fn finish(self) -> Result<RunningEvals, WarmupError> {
        let evals = self
            .evals
            .await
            .map_err(|_| WarmupError::ExtractThreadCrashed)??;

        Ok(evals)
    }
}
