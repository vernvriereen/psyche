use crate::fetch_data::Batch;
use anyhow::{bail, Error, Result};
use psyche_coordinator::model;
use psyche_core::LearningRateScheduler;
use psyche_modeling::{CausalLM, Distro, DistroResult, LlamaForCausalLM};
use std::sync::mpsc;
use tch::{
    nn::{self, OptimizerConfig},
    Tensor,
};
use tracing::{debug, error};

pub type ParallelModels = Vec<LlamaForCausalLM>;

enum Optimizer {
    AdamW {
        optimizer: nn::Optimizer,
        clip_grad_norm: Option<f32>,
    },
    Distro(Distro),
}

pub type DistroResults = Vec<DistroResult>;

pub struct TrainOutput {
    pub trainer: Trainer,
    pub loss: f32,
    pub step: usize,
    pub distro_results: DistroResults,
}

enum ParallelAssignment {
    Train {
        data: Batch,
        lr: f64,
    },
    Optimize {
        distro_results: Option<Vec<DistroResults>>,
        lr: f64,
    },
}

enum ParallelResult {
    Train {
        loss: f32,
        distro_results: Option<Vec<DistroResult>>,
    },
    Optimize {},
}

pub struct Trainer {
    models: Vec<(
        mpsc::Sender<ParallelAssignment>,
        mpsc::Receiver<ParallelResult>,
    )>,
    lr_scheduler: Box<dyn LearningRateScheduler>,
}

impl Trainer {
    pub fn new(
        models: ParallelModels,
        lr_scheduler: Box<dyn LearningRateScheduler>,
        optimizer: model::Optimizer,
    ) -> Self {
        let mut ret = Vec::with_capacity(models.len());
        for (index, model) in models.into_iter().enumerate() {
            let (assignment_tx, assignment_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            ret.push((assignment_tx, result_rx));

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
                    index,
                    model.comm.clone(),
                )),
            };

            std::thread::spawn(move || {
                Self::model_thread(model, assignment_rx, result_tx, optimizer, index)
            });
        }
        Self {
            models: ret,
            lr_scheduler,
        }
    }

    fn forward_backward(model: &mut LlamaForCausalLM, data: &Batch) -> Result<f32> {
        let inputs = Tensor::from_slice2(data).to(model.device());
        let targets = inputs.copy();
        let (_, loss) = model.forward(&inputs, Some(&targets), None);
        let loss = loss.ok_or(Error::msg("No loss"))?;
        loss.backward();
        Ok(loss.try_into()?)
    }

    pub fn train(self, step: usize, data: Batch) -> Result<TrainOutput> {
        let lr: f64 = self.lr_scheduler.get_lr(step);
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Train {
                data: data.clone(),
                lr: lr,
            })
            .map_err(|err| Error::msg(format!("Error sending batch to trainer thread: {err}")))?;
        }
        let mut final_loss = 0.0;
        let mut final_distro_results = None;
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Train {
                    loss,
                    distro_results,
                } => {
                    if final_distro_results.is_none() {
                        final_distro_results = distro_results;
                    }
                    final_loss += loss;
                }
                ParallelResult::Optimize {} => bail!("Got unexpected optimizer result"),
            }
        }
        final_loss /= self.models.len() as f32;
        Ok(TrainOutput {
            trainer: self,
            loss: final_loss,
            step,
            distro_results: final_distro_results.unwrap_or_default(),
        })
    }

    pub fn apply_distro_results(
        self,
        step: usize,
        results: Vec<Vec<DistroResult>>,
    ) -> Result<Self> {
        let lr: f64 = self.lr_scheduler.get_lr(step);
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Optimize {
                distro_results: Some(results.clone()),
                lr,
            })
            .map_err(|err| {
                Error::msg(format!(
                    "Error sending optimization to trainer thread: {err}"
                ))
            })?;
        }
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Train {
                    loss: _,
                    distro_results: _,
                } => bail!("Got unexpected trainer result"),
                ParallelResult::Optimize {} => {}
            }
        }
        Ok(self)
    }

    fn model_thread(
        mut model: LlamaForCausalLM,
        assignment: mpsc::Receiver<ParallelAssignment>,
        submission: mpsc::Sender<ParallelResult>,
        mut optimizer: Optimizer,
        index: usize,
    ) {
        loop {
            match assignment.recv() {
                Ok(ParallelAssignment::Train { data, lr }) => {
                    match Self::forward_backward(&mut model, &data) {
                        Ok(loss) => {
                            let distro_results = match &mut optimizer {
                                Optimizer::AdamW {
                                    optimizer: _,
                                    clip_grad_norm: _,
                                } => None,
                                Optimizer::Distro(distro) => {
                                    let ret = distro.generate(lr);
                                    // this is a gpu p2p optimization -- only the first gpu really produces results,
                                    // the other gpus merely feed their tp tensors to the first rank
                                    match index == 0 {
                                        true => Some(ret),
                                        false => None,
                                    }
                                }
                            };
                            if submission
                                .send(ParallelResult::Train {
                                    loss,
                                    distro_results,
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(err) => {
                            error!("Train error: {err}");
                            return;
                        }
                    }
                }
                Ok(ParallelAssignment::Optimize { distro_results, lr }) => {
                    match &mut optimizer {
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
                        }
                        Optimizer::Distro(distro) => match distro_results {
                            Some(results) => {
                                debug!("Applying {} DisTrO gradients", results.len());
                                distro.apply(results, lr);
                            }
                            None => {
                                error!("Got DisTrO optimizer assignment, but no results");
                                return;
                            }
                        },
                    };
                    if submission.send(ParallelResult::Optimize {}).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    return;
                }
            }
        }
    }
}
