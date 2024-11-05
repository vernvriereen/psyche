use psyche_core::LearningRateScheduler;
use psyche_serde::derive_serialize;

#[cfg(target_os = "solana")]
use anchor_lang::prelude::*;
#[cfg(not(target_os = "solana"))]
use serde::{Deserialize, Serialize};

#[derive_serialize]
#[derive(Clone, Debug)]
pub enum Model {
    LLM(LLM),
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub enum LLMArchitecture {
    HfLlama,
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub enum LLMTrainingDataType {
    Pretraining,
    Finetuning,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub enum LLMTrainingDataLocation {
    Server(String),
    Local(String),
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub struct ConstantLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub struct LinearLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub struct CosineLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub enum LearningRateSchedule {
    Constant(ConstantLR),
    Linear(LinearLR),
    Cosine(CosineLR),
}

#[derive_serialize]
#[derive(Copy, Clone, Debug)]
pub enum Optimizer {
    AdamW {
        betas: [f32; 2],
        weight_decay: f32,
        eps: f32,
        clip_grad_norm: Option<f32>,
    },
    Distro {
        compression_decay: f32,
        compression_topk: u16,
        compression_topk_startup: u16,
        compression_topk_startup_steps: u32,
        compression_chunk: u16,
    },
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct LLM {
    pub architecture: LLMArchitecture,
    pub checkpoint: Checkpoint,
    pub max_seq_len: u32,
    pub data_type: LLMTrainingDataType,
    pub data_location: LLMTrainingDataLocation,
    pub lr_schedule: LearningRateSchedule,
    pub optimizer: Optimizer,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct HubRepo {
    pub repo_id: String,
    pub revision: Option<String>,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub enum Checkpoint {
    Ephemeral,
    Hub(HubRepo),
}

impl From<ConstantLR> for psyche_core::ConstantLR {
    fn from(value: ConstantLR) -> Self {
        psyche_core::ConstantLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
        )
    }
}

impl From<LinearLR> for psyche_core::LinearLR {
    fn from(value: LinearLR) -> Self {
        psyche_core::LinearLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
            value.total_steps,
            value.final_lr as f64,
        )
    }
}

impl From<CosineLR> for psyche_core::CosineLR {
    fn from(value: CosineLR) -> Self {
        psyche_core::CosineLR::new(
            value.base_lr as f64,
            value.warmup_steps,
            value.warmup_init_lr as f64,
            value.total_steps,
            value.final_lr as f64,
        )
    }
}

// TODO why not unify the values here and in core?
#[derive(Clone)]
pub enum AnyLearningRateScheduler {
    Constant(psyche_core::ConstantLR),
    Linear(psyche_core::LinearLR),
    Cosine(psyche_core::CosineLR),
}

impl AnyLearningRateScheduler {
    pub fn get_lr(&self, step: u32) -> f64 {
        match self {
            Self::Constant(l) => l.get_lr(step),
            Self::Linear(l) => l.get_lr(step),
            Self::Cosine(l) => l.get_lr(step),
        }
    }

    pub fn in_warmup(&self, step: u32) -> bool {
        match self {
            Self::Constant(l) => l.in_warmup(step),
            Self::Linear(l) => l.in_warmup(step),
            Self::Cosine(l) => l.in_warmup(step),
        }
    }
}

impl From<LearningRateSchedule> for AnyLearningRateScheduler {
    fn from(value: LearningRateSchedule) -> Self {
        match value {
            LearningRateSchedule::Constant(c) => Self::Constant(c.into()),
            LearningRateSchedule::Linear(c) => Self::Linear(c.into()),
            LearningRateSchedule::Cosine(c) => Self::Cosine(c.into()),
        }
    }
}
