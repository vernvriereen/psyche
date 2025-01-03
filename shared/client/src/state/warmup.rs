use super::evals::{EvalRunner, MaybeRunningEvals, RunningEvals};

pub struct WarmupStepMetadata {
    pub eval_runner: EvalRunner,
}

impl WarmupStepMetadata {
    pub fn start(&self, evals_or_trainers: impl Into<MaybeRunningEvals>) -> WarmupStep {
        let evals = self
            .eval_runner
            .start_if_not_running(evals_or_trainers.into());
        WarmupStep { evals }
    }
}

#[derive(Debug)]
pub struct WarmupStep {
    evals: RunningEvals,
}

impl WarmupStep {
    pub fn finish(self) -> RunningEvals {
        self.evals
    }
}
