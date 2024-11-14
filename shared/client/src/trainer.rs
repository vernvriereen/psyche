use crate::fetch_data::Batch;
use anyhow::{bail, Error, Result};
use psyche_coordinator::{
    model::{self, AnyLearningRateScheduler},
    RunState,
};
use psyche_core::CancellableBarrier;
use psyche_modeling::{
    unsharded_cpu_variables, CausalLM, Distro, DistroResult, Fp32GradientAccumulator,
    LlamaForCausalLM,
};
use std::{
    collections::HashMap,
    ops::ControlFlow,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::Instant,
};
use tch::{
    nn::{self, OptimizerConfig},
    Device, Tensor,
};
use thiserror::Error;
use tracing::{debug, error};

pub type ParallelModels = Vec<LlamaForCausalLM>;

enum Optimizer {
    AdamW {
        optimizer: nn::Optimizer,
        clip_grad_norm: Option<f32>,
    },
    Distro {
        optimizer: Box<Distro>,
        clip_grad_norm: Option<f32>,
        compression_decay_warmup_steps: u32,
        compression_topk: i64,
        compression_topk_startup: i64,
        compression_topk_startup_steps: u32,
        quantize: bool,
    },
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
    Forward {
        data: Tensor,
        labels: Option<Tensor>,
        num_logits_to_keep: Option<i64>,
    },
    Extract {},
}

#[derive(Debug)]
enum ParallelResult {
    Train {
        loss: f32,
        cancelled: bool,
        distro_results: Option<DistroResults>,
    },
    Optimize,
    Forward {
        logits_and_loss: Option<(Tensor, Option<Tensor>)>,
    },
    Extract {
        variables: HashMap<String, Tensor>,
    },
}

pub struct Trainer {
    models: Vec<(
        mpsc::Sender<ParallelAssignment>,
        mpsc::Receiver<ParallelResult>,
    )>,
    first_model_device: Device,
    barrier: Arc<CancellableBarrier>,
}

impl Trainer {
    pub fn new(
        models: ParallelModels,
        lr_scheduler: AnyLearningRateScheduler,
        optimizer: model::Optimizer,
        micro_batch_size: usize,
        run_state: Arc<AtomicUsize>,
        stats: bool,
    ) -> Self {
        assert!(!models.is_empty());
        let first_model_device = models[0].device();
        let mut ret = Vec::with_capacity(models.len());
        let barrier = CancellableBarrier::new(models.len());
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
                    clip_grad_norm,
                    compression_decay,
                    compression_decay_warmup_steps,
                    compression_topk,
                    compression_topk_startup,
                    compression_topk_startup_steps,
                    compression_chunk,
                    quantize,
                } => Optimizer::Distro {
                    optimizer: Distro::new(
                        &model.variables,
                        compression_decay as f64,
                        compression_chunk as i64,
                        0.0,
                        model.comm.clone(),
                        stats,
                    )
                    .into(),
                    clip_grad_norm,
                    compression_decay_warmup_steps,
                    compression_topk: compression_topk as i64,
                    compression_topk_startup: compression_topk_startup as i64,
                    compression_topk_startup_steps,
                    quantize,
                },
            };

            let run_state = run_state.clone();
            let lr_scheduler = lr_scheduler.clone();
            let barrier = barrier.clone();
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
                    barrier,
                )
            });
        }
        Self {
            models: ret,
            first_model_device,
            barrier,
        }
    }

    fn forward_backward(
        model: &mut LlamaForCausalLM,
        data: &[Vec<i32>],
        barrier: &Arc<CancellableBarrier>,
        loss_scale: Option<f64>,
    ) -> Result<Option<f32>> {
        let device = model.device();
        let inputs = Tensor::from_slice2(data).to(device);
        let targets = inputs.copy();
        if barrier.wait().is_err() {
            return Ok(None);
        }
        let (_, loss) = model.forward(&inputs, Some(&targets), None);
        let mut loss = loss.ok_or(Error::msg("No loss"))?;
        if let Some(loss_scale) = loss_scale {
            loss /= loss_scale;
        }
        loss.backward();
        if barrier.wait().is_err() {
            return Ok(None);
        }
        Ok(Some(loss.try_into()?))
    }

    fn forward(
        model: &mut LlamaForCausalLM,
        data: &Tensor,
        labels: Option<&Tensor>,
        barrier: &Arc<CancellableBarrier>,
        num_logits_to_keeep: Option<i64>,
    ) -> Result<Option<(Tensor, Option<Tensor>)>> {
        let _guard = tch::no_grad_guard();
        let device = model.device();
        let inputs = data.to(device);
        let labels = labels.map(|x| x.to(device));
        if barrier.wait().is_err() {
            return Ok(None);
        }
        let (logits, loss) = model.forward(&inputs, labels.as_ref(), num_logits_to_keeep);
        if barrier.wait().is_err() {
            return Ok(None);
        }
        Ok(Some((logits, loss)))
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
        self.barrier.reset();
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
                _ => bail!("Got unexpected ParallelResult in train()"),
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

    pub fn apply_distro_results(
        self,
        step: u32,
        results: Vec<DistroResults>,
    ) -> Result<Self, ApplyDistroResultError> {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Optimize {
                distro_results: Some(results.clone()),
                step,
            })
            .map_err(|_| ApplyDistroResultError::SendOptimize)?;
        }
        let start = Instant::now();
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Optimize => {
                    debug!(
                        "ParallelResult::Optimize received in {}s",
                        (Instant::now() - start).as_secs_f32()
                    );
                }
                o => {
                    return Err(ApplyDistroResultError::RecievedWrongResultType(format!(
                        "{o:?}"
                    )))
                }
            }
        }
        Ok(self)
    }

    pub fn extract(self) -> Result<(HashMap<String, Tensor>, Self)> {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Extract {}).map_err(|err| {
                Error::msg(format!("Error sending extraction to trainer thread: {err}"))
            })?;
        }
        let mut extracted = HashMap::new();
        for (_, rx) in &self.models {
            match rx.recv()? {
                ParallelResult::Extract { variables } => {
                    if extracted.is_empty() && !variables.is_empty() {
                        extracted = variables;
                    }
                }
                _ => bail!("Got unexpected ParallelResult in extract()"),
            }
        }
        Ok((extracted, self))
    }

    // todo: refactor args into a struct
    #[allow(clippy::too_many_arguments)]
    fn model_thread(
        mut model: LlamaForCausalLM,
        assignment: mpsc::Receiver<ParallelAssignment>,
        submission: mpsc::Sender<ParallelResult>,
        mut optimizer: Optimizer,
        index: usize,
        micro_batch_size: usize,
        run_state: Arc<AtomicUsize>,
        lr_scheduler: AnyLearningRateScheduler,
        barrier: Arc<CancellableBarrier>,
    ) {
        if let Err(err) = Self::forward_backward(&mut model, &[vec![0i32]], &barrier, None) {
            error!("Test forward/backward gave error {err}");
            return;
        }
        let mut grad_accum: Option<Fp32GradientAccumulator> = None;
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
                        if optimize_step(lr, &mut optimizer, Some(result), &barrier).is_break() {
                            panic!("Failed to roll back.")
                        };
                    }

                    if micro_batch_size > 0 && data.len() % micro_batch_size != 0 {
                        error!("Micro batch size doesn't evenly divide batch size");
                        return;
                    }

                    let grad_accum_steps = data.len() / micro_batch_size;
                    if grad_accum_steps != 1 && grad_accum.is_none() {
                        debug!("Allocating FP32 gradient accumulator");
                        let parameters = match &mut optimizer {
                            Optimizer::AdamW { optimizer, .. } => optimizer.trainable_variables(),
                            Optimizer::Distro { optimizer, .. } => optimizer.trainable_variables(),
                        };
                        grad_accum = Some(Fp32GradientAccumulator::new(&parameters, model.device()))
                    }
                    let grad_accum_divisor = grad_accum_steps as f64;
                    let micro_batches = data.chunks_exact(micro_batch_size);
                    assert_eq!(micro_batches.len(), grad_accum_steps);
                    match &mut grad_accum {
                        Some(grad_accum) => grad_accum.zero_grad(),
                        None => match &mut optimizer {
                            Optimizer::AdamW { optimizer, .. } => optimizer.zero_grad(),
                            Optimizer::Distro { optimizer, .. } => optimizer.zero_grad(),
                        },
                    };

                    let mut loss = 0f32;
                    let mut cancelled = false;
                    for micro_batch in micro_batches {
                        if RunState::try_from(run_state.load(Ordering::Relaxed)).unwrap()
                            != RunState::RoundTrain
                        {
                            cancelled = true;
                            barrier.cancel();
                            debug!("Aborting training, run state changed");
                            break;
                        }
                        match Self::forward_backward(
                            &mut model,
                            micro_batch,
                            &barrier,
                            Some(grad_accum_divisor),
                        ) {
                            Ok(Some(batch_loss)) => loss += batch_loss,
                            Ok(None) => {
                                // cancelled barrier catching race to on run_state
                                cancelled = true;
                                debug!("Aborting training, run state changed");
                                break;
                            }
                            Err(err) => {
                                error!("Train error: {err}");
                                return;
                            }
                        }
                        if let Some(grad_accum) = &mut grad_accum {
                            grad_accum.accumulate_gradients();
                        }
                    }
                    if let Some(grad_accum) = &mut grad_accum {
                        grad_accum.apply_accumulation();
                    }
                    let distro_results = match cancelled {
                        false => match &mut optimizer {
                            Optimizer::AdamW {
                                optimizer: _,
                                clip_grad_norm: _,
                            } => None,
                            Optimizer::Distro {
                                optimizer,
                                clip_grad_norm,
                                compression_decay_warmup_steps,
                                compression_topk,
                                compression_topk_startup,
                                compression_topk_startup_steps,
                                quantize,
                            } => {
                                let clipped = match clip_grad_norm {
                                    Some(clip_grad_norm) => match barrier.wait() {
                                        Ok(_) => {
                                            optimizer.clip_grad_norm(*clip_grad_norm as f64);
                                            barrier.wait().is_ok()
                                        }
                                        Err(_) => false,
                                    },
                                    None => true,
                                };
                                if clipped {
                                    let ret = optimizer.generate(
                                        lr_scheduler.get_lr(step),
                                        match step > *compression_decay_warmup_steps {
                                            true => 1.0,
                                            false => {
                                                step as f64 / *compression_decay_warmup_steps as f64
                                            }
                                        },
                                        match step <= *compression_topk_startup_steps {
                                            true => *compression_topk_startup,
                                            false => *compression_topk,
                                        },
                                        *quantize,
                                    );
                                    // just need results from one of the ranks
                                    match index == 0 {
                                        true => Some(ret),
                                        false => None,
                                    }
                                } else {
                                    cancelled = true;
                                    None
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
                        if optimize_step(lr, &mut optimizer, Some(result), &barrier).is_break() {
                            panic!("Failed to roll forwards.")
                        };
                    }
                }
                Ok(ParallelAssignment::Optimize {
                    distro_results,
                    step,
                }) => {
                    let lr = lr_scheduler.get_lr(step);
                    if optimize_step(lr, &mut optimizer, distro_results.as_ref(), &barrier)
                        .is_break()
                    {
                        return;
                    }
                    if submission.send(ParallelResult::Optimize).is_err() {
                        return;
                    }
                }
                Ok(ParallelAssignment::Forward {
                    data,
                    labels,
                    num_logits_to_keep,
                }) => {
                    let logits_and_loss = match Self::forward(
                        &mut model,
                        &data,
                        labels.as_ref(),
                        &barrier,
                        num_logits_to_keep,
                    ) {
                        Ok(Some(logits_and_loss)) => Some(logits_and_loss),
                        Ok(None) => None,
                        Err(err) => {
                            error!("Unexpected error in forward: {err}");
                            return;
                        }
                    };
                    if submission
                        .send(ParallelResult::Forward { logits_and_loss })
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(ParallelAssignment::Extract {}) => {
                    match unsharded_cpu_variables(&model.variables, model.comm.clone()) {
                        Ok(variables) => {
                            if submission
                                .send(ParallelResult::Extract { variables })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(err) => {
                            error!("Unexpected error in extract: {err}");
                            return;
                        }
                    }
                }
                Err(_) => {
                    return;
                }
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum ApplyDistroResultError {
    #[error("failed to send optimization to trainer thread - trainer thread RX is closed")]
    SendOptimize,

    #[error("failed to recv optimization result from trainer thread: {0}")]
    ReceiveResult(#[from] std::sync::mpsc::RecvError),

    #[error("recieved wrong result type from trainer thread. expected Optimize, got {0:?}")]
    RecievedWrongResultType(String),
}

impl CausalLM for Trainer {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>) {
        self.barrier.reset();
        for (tx, _) in &self.models {
            tx.send(ParallelAssignment::Forward {
                data: x.shallow_clone(),
                labels: labels.map(|y| y.shallow_clone()),
                num_logits_to_keep,
            })
            .expect("Error getting result from forward");
        }
        let mut final_logits_and_loss = None;
        for (_, rx) in &self.models {
            match rx.recv() {
                Ok(ParallelResult::Forward { logits_and_loss }) => {
                    if final_logits_and_loss.is_none() {
                        final_logits_and_loss = logits_and_loss;
                    }
                }
                _ => panic!("Got unexpected forward result"),
            }
        }
        final_logits_and_loss.expect("No forward logits and loss")
    }

    fn bos_token_id(&self) -> Option<i64> {
        None
    }

    fn device(&self) -> tch::Device {
        self.first_model_device
    }
}

fn optimize_step(
    lr: f64,
    optimizer: &mut Optimizer,
    distro_results: Option<&Vec<Vec<DistroResult>>>,
    barrier: &Arc<CancellableBarrier>,
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
        Optimizer::Distro { optimizer, .. } => match distro_results {
            Some(results) => {
                if !results.is_empty() {
                    debug!("Applying {} DisTrO gradients", results.len());
                } else {
                    error!("Empty DisTrO gradients");
                    return ControlFlow::Break(());
                }
                if barrier.wait().is_err() {
                    return ControlFlow::Break(());
                }
                optimizer.apply(results, lr);
                if barrier.wait().is_err() {
                    return ControlFlow::Break(());
                }
            }
            None => {
                error!("Got DisTrO optimizer assignment, but no results");
                return ControlFlow::Break(());
            }
        },
    };
    ControlFlow::Continue(())
}
