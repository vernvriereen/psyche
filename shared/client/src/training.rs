use anyhow::Result;
use psyche_coordinator::model;
use psyche_core::LearningRateScheduler;
use psyche_modeling::LlamaForCausalLM;

pub struct Trainer {
    _model: LlamaForCausalLM,
}

impl Trainer {
    pub fn new(model: LlamaForCausalLM) -> Self {
        Self { _model: model }
    }

    pub async fn train(
        self,
        _lr_schedule: Box<dyn LearningRateScheduler>,
        _optimizer: model::Optimizer,
        _data: Vec<Vec<i32>>,
    ) -> Result<Trainer> {
        Ok(self)
    }
}
