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
#[derive(Clone, Debug)]
pub enum LLMArchitecture {
    HfLlama,
}

#[derive_serialize]
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct ConstantLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct LinearLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct CosineLR {
    base_lr: f32,
    warmup_steps: u32,
    warmup_init_lr: f32,
    total_steps: u32,
    final_lr: f32,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub enum LearningRateSchedule {
    Constant(ConstantLR),
    Linear(LinearLR),
    Cosine(CosineLR),
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub enum Optimizer {
    Distro {
        betas: [f32; 3],
        weight_decay: f32,
        eps: f32,
        compression_topk: u16,
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
    Hub(HubRepo),
}

impl From<ConstantLR> for psyche_core::ConstantLR {
    fn from(value: ConstantLR) -> Self {
        psyche_core::ConstantLR::new(
            value.base_lr as f64,
            value.warmup_steps as usize,
            value.warmup_init_lr as f64,
        )
    }
}

impl From<LinearLR> for psyche_core::LinearLR {
    fn from(value: LinearLR) -> Self {
        psyche_core::LinearLR::new(
            value.base_lr as f64,
            value.warmup_steps as usize,
            value.warmup_init_lr as f64,
            value.total_steps as usize,
            value.final_lr as f64,
        )
    }
}

impl From<CosineLR> for psyche_core::CosineLR {
    fn from(value: CosineLR) -> Self {
        psyche_core::CosineLR::new(
            value.base_lr as f64,
            value.warmup_steps as usize,
            value.warmup_init_lr as f64,
            value.total_steps as usize,
            value.final_lr as f64,
        )
    }
}
