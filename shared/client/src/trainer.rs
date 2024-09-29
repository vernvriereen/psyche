use anyhow::{Error, Result};
use psyche_coordinator::model;
use psyche_core::LearningRateScheduler;
use psyche_modeling::{CausalLM, LlamaForCausalLM};
use tch::{
    nn::{self, OptimizerConfig},
    Tensor,
};
use tracing::debug;

enum Optimizer {
    AdamW {
        optimizer: nn::Optimizer,
        clip_grad_norm: Option<f32>,
    },
}

pub struct TrainOutput {
    pub trainer: Trainer,
    pub _loss: f32,
    pub step: usize,
}

pub struct Trainer {
    model: LlamaForCausalLM,
    lr_scheduler: Box<dyn LearningRateScheduler>,
    optimizer: Optimizer,
}

impl Trainer {
    pub fn new(
        model: LlamaForCausalLM,
        lr_scheduler: Box<dyn LearningRateScheduler>,
        optimizer: model::Optimizer,
    ) -> Self {
        let optimizer = match optimizer {
            model::Optimizer::AdamW {
                betas,
                weight_decay,
                eps,
                clip_grad_norm,
            } => Optimizer::AdamW {
                optimizer: nn::AdamW {
                    beta1: betas[0] as f64,
                    beta2: betas[1] as f64,
                    wd: weight_decay as f64,
                    eps: eps as f64,
                    amsgrad: false,
                }
                .build(&model.variables, 1.0e-1)
                .unwrap(),
                clip_grad_norm,
            },
            model::Optimizer::Distro {
                compression_decay: _,
                compression_topk: _,
                compression_chunk: _,
            } => todo!(),
        };
        Self {
            model,
            lr_scheduler,
            optimizer,
        }
    }

    pub fn train(mut self, step: usize, data: Vec<Vec<i32>>) -> Result<TrainOutput> {
        let inputs = Tensor::from_slice2(&data).to(self.model.device());
        let targets = inputs.copy();
        let (_, loss) = self.model.forward(&inputs, Some(&targets), None);
        let loss = loss.ok_or(Error::msg("No loss"))?;
        loss.backward();
        let loss_value: f32 = loss.try_into()?;
        match &mut self.optimizer {
            Optimizer::AdamW {
                optimizer,
                clip_grad_norm,
            } => {
                optimizer.set_lr(self.lr_scheduler.get_lr(step));
                if let Some(clip_grad_norm) = clip_grad_norm {
                    optimizer.clip_grad_norm(*clip_grad_norm as f64);
                }
                optimizer.step();
                optimizer.zero_grad();
            }
        }
        debug!("step: {step}, loss: {loss_value}");
        Ok(TrainOutput {
            trainer: self,
            _loss: loss_value,
            step,
        })
    }
}
