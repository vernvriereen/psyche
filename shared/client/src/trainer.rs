use std::io::Cursor;

use anyhow::{bail, Error, Result};
use psyche_coordinator::model;
use psyche_core::LearningRateScheduler;
use psyche_modeling::{CausalLM, Distro, DistroResult, LlamaForCausalLM};
use serde::{Deserialize, Serialize};
use tch::{
    nn::{self, OptimizerConfig},
    Tensor,
};
use tracing::{debug, info};

enum Optimizer {
    AdamW {
        optimizer: nn::Optimizer,
        clip_grad_norm: Option<f32>,
    },
    Distro(Distro),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SerializedDistroResult {
    pub sparse_idx: Vec<u8>,
    pub sparse_val: Vec<u8>,
    pub xshape: Vec<u16>,
}

pub struct TrainOutput {
    pub trainer: Trainer,
    pub loss: f32,
    pub step: usize,
    pub distro_results: Vec<DistroResult>,
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
                compression_decay,
                compression_topk,
                compression_chunk,
            } => Optimizer::Distro(Distro::new(
                &model.variables,
                compression_decay as f64,
                compression_chunk as i64,
                compression_topk as i64,
                0.0,
            )),
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
        let lr = self.lr_scheduler.get_lr(step);
        let distro_results = match &mut self.optimizer {
            Optimizer::AdamW {
                optimizer,
                clip_grad_norm,
            } => {
                optimizer.set_lr(lr);
                if let Some(clip_grad_norm) = clip_grad_norm {
                    optimizer.clip_grad_norm(*clip_grad_norm as f64);
                }
                optimizer.step();
                optimizer.zero_grad();
                Vec::new()
            }
            Optimizer::Distro(distro) => distro.generate(lr),
        };
        debug!("Step: {step}, Loss: {loss_value}");
        Ok(TrainOutput {
            trainer: self,
            loss: loss_value,
            step,
            distro_results,
        })
    }

    pub fn apply_distro_results(
        mut self,
        step: usize,
        results: Vec<Vec<DistroResult>>,
    ) -> Result<Self> {
        match &mut self.optimizer {
            Optimizer::AdamW {
                optimizer: _,
                clip_grad_norm: _,
            } => {
                bail!("Not DisTrO");
            }
            Optimizer::Distro(distro) => {
                debug!("Applying {} DisTrO gradients", results.len());
                distro.apply(results, self.lr_scheduler.get_lr(step));
            }
        }
        Ok(self)
    }
}

fn serialize_tensor(tensor: &Tensor) -> Vec<u8> {
    let mut buffer = Vec::new();
    tensor.save_to_stream(&mut buffer).unwrap();
    buffer
}

impl From<&DistroResult> for SerializedDistroResult {
    fn from(value: &DistroResult) -> Self {
        let sparse_idx = serialize_tensor(&value.sparse_idx);
        info!("sparse_idx: {}", value.sparse_idx);
        info!("serialized sparse_idx: {} bytes", sparse_idx.len());
        Self {
            sparse_idx,
            sparse_val: serialize_tensor(&value.sparse_val),
            xshape: value.xshape.iter().map(|x| *x as u16).collect(),
        }
    }
}

impl TryFrom<SerializedDistroResult> for DistroResult {
    type Error = tch::TchError;

    fn try_from(value: SerializedDistroResult) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            sparse_idx: Tensor::load_from_stream(Cursor::new(value.sparse_idx))?,
            sparse_val: Tensor::load_from_stream(Cursor::new(value.sparse_val))?,
            xshape: value.xshape.iter().map(|x| *x as i64).collect(),
        })
    }
}
