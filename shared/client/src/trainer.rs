use crate::fetch_data::Batch;
use anyhow::{bail, Error, Result};
use psyche_coordinator::model::{self, AnyLearningRateScheduler};
use psyche_coordinator::RunState;
use psyche_modeling::{CausalLM, Distro, DistroResult, LlamaForCausalLM};
use std::ops::ControlFlow;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::Instant;
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
    pub step: u32,
    pub distro_results: DistroResults,
    pub cancelled: bool,
}

enum ParallelAssignment {
    Train {
        data: Batch,
        step: u32,
        rollback: Vec<(u32, Vec<DistroResults>)>,
    },
    Optimize {
        distro_results: Option<Vec<DistroResults>>,
        step: u32,
    },
}

enum ParallelResult {
    Train {
        loss: f32,
        cancelled: bool,
        distro_results: Option<DistroResults>,
    },
    Optimize,
}

pub struct Trainer {
    models: Vec<(
        mpsc::Sender<ParallelAssignment>,
        mpsc::Receiver<ParallelResult>,
    )>,
}

impl Trainer {
    pub fn new(
        models: ParallelModels,
        lr_scheduler: AnyLearningRateScheduler,
        optimizer: model::Optimizer,
        micro_batch_size: usize,
        run_state: Arc<AtomicUsize>,
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

            let run_state = run_state.clone();
            let lr_scheduler = lr_scheduler.clone();
            std::thread::spawn(move || {
                Self::model_thread(
                    model,
                    assignment_rx,
                    result_tx,
                    optimizer,
                    index,
                    micro_batch_size,
                    run_state,
                    lr_scheduler,
                )
            });
        }
        Self { models: ret }
    }

    fn forward_backward(model: &mut LlamaForCausalLM, data: &[Vec<i32>]) -> Result<f32> {
        let inputs = Tensor::from_slice2(data).to(model.device());
        let targets = inputs.copy();
        let (_, loss) = model.forward(&inputs, Some(&targets), None);
        let loss = loss.ok_or(Error::msg("No loss"))?;
        loss.backward();
        Ok(loss.try_into()?)
    }

    pub fn train(
        self,
        step: u32,
        data: Batch,
        rollback: Vec<(u32, Vec<DistroResults>)>,
    ) -> Result<TrainOutput> {
        if !rollback.is_empty() {
            error!(
                "we have not implemented getting data from previous rounds. this should be impossible to hit.. this step is {step}, rollback passed is {:?}",
                rollback.iter().map(|(step, _)| step).collect::<Vec<_>>());
        }
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Train {
                data: data.clone(),
                step,
                rollback: rollback.clone(),
            })
            .map_err(|err| Error::msg(format!("Error sending batch to trainer thread: {err}")))?;
        }
        let mut final_loss = 0.0;
        let mut final_distro_results = None;
        let mut final_cancelled = false;
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Train {
                    loss,
                    distro_results,
                    cancelled,
                } => {
                    if final_distro_results.is_none() {
                        final_distro_results = distro_results;
                    }
                    final_cancelled = cancelled;
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
            cancelled: final_cancelled,
        })
    }

    pub fn apply_distro_results(self, step: u32, results: Vec<DistroResults>) -> Result<Self> {
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Optimize {
                distro_results: Some(results.clone()),
                step,
            })
            .map_err(|err| {
                Error::msg(format!(
                    "Error sending optimization to trainer thread: {err}"
                ))
            })?;
        }
        let start = Instant::now();
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Train {
                    loss: _,
                    distro_results: _,
                    cancelled: _,
                } => bail!("Got unexpected trainer result"),
                ParallelResult::Optimize {} => {
                    debug!(
                        "ParallelResult::Optimize received in {}s",
                        (Instant::now() - start).as_secs_f32()
                    );
                }
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
        micro_batch_size: usize,
        run_state: Arc<AtomicUsize>,
        lr_scheduler: AnyLearningRateScheduler,
    ) {
        loop {
            match assignment.recv() {
                Ok(ParallelAssignment::Train {
                    data,
                    step,
                    rollback,
                }) => {
                    for (step, result) in rollback.iter().rev() {
                        // TODO freeze the optimizer and prevent this from modifying its internal state, methinks, right? or maybe save it and restore it later?
                        // we just want to have the same optimizer state wyhen we exit, save for the main operation (if not frozen. hmm)
                        let lr = lr_scheduler.get_lr(*step);
                        if optimize_step(lr, &mut optimizer, Some(result)).is_break() {
                            panic!("Failed to roll back.")
                        };
                    }

                    if micro_batch_size > 0 && data.len() % micro_batch_size != 0 {
                        error!("Micro batch size doesn't evenly divide batch size");
                        return;
                    }
                    let grad_accum_steps = data.len() / micro_batch_size;
                    let grad_accum_divisor = grad_accum_steps as f32;
                    let micro_batches = data.chunks_exact(micro_batch_size);
                    assert_eq!(micro_batches.len(), grad_accum_steps);
                    let mut loss = 0f32;
                    let mut cancelled = false;
                    for micro_batch in micro_batches {
                        if RunState::try_from(run_state.load(Ordering::Relaxed)).unwrap()
                            != RunState::RoundTrain
                        {
                            cancelled = true;
                            debug!("Aborting training, run state changed");
                            break;
                        }
                        match Self::forward_backward(&mut model, micro_batch) {
                            Ok(batch_loss) => loss += batch_loss,
                            Err(err) => {
                                error!("Train error: {err}");
                                return;
                            }
                        }
                    }
                    loss /= grad_accum_divisor;
                    let distro_results = match cancelled {
                        false => match &mut optimizer {
                            Optimizer::AdamW {
                                optimizer: _,
                                clip_grad_norm: _,
                            } => None,
                            Optimizer::Distro(distro) => {
                                let lr = lr_scheduler.get_lr(step);
                                let ret = distro.generate(
                                    lr,
                                    run_state.clone(),
                                    RunState::RoundTrain.into(),
                                );
                                if ret.is_none() {
                                    cancelled = true;
                                    debug!("Aborting DisTrO generation, run state changed");
                                }
                                // this is a gpu p2p optimization -- only the first gpu really produces results,
                                // the other gpus merely feed their tp tensors to the first rank
                                match index == 0 {
                                    true => ret,
                                    false => None,
                                }
                            }
                        },
                        true => None,
                    };
                    if submission
                        .send(ParallelResult::Train {
                            loss,
                            distro_results,
                            cancelled,
                        })
                        .is_err()
                    {
                        return;
                    }

                    for (step, result) in rollback.iter() {
                        // TODO freeze the optimizer and prevent this from modifying its internal state, methinks, right? or maybe save it and restore it later?
                        // we just want to have the same optimizer state wyhen we exit, save for the main operation (if not frozen. hmm)
                        let lr = lr_scheduler.get_lr(*step);
                        if optimize_step(lr, &mut optimizer, Some(result)).is_break() {
                            panic!("Failed to roll forwards.")
                        };
                    }
                }
                Ok(ParallelAssignment::Optimize {
                    distro_results,
                    step,
                }) => {
                    let lr = lr_scheduler.get_lr(step);
                    if optimize_step(lr, &mut optimizer, distro_results.as_ref()).is_break() {
                        return;
                    }
                    if submission.send(ParallelResult::Optimize).is_err() {
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

// TODO impl freezing? :)
#[must_use]
fn optimize_step(
    lr: f64,
    optimizer: &mut Optimizer,
    distro_results: Option<&Vec<Vec<DistroResult>>>,
) -> ControlFlow<()> {
    match optimizer {
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
                return ControlFlow::Break(());
            }
        },
    };
    ControlFlow::Continue(())
}
