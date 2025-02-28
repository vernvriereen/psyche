use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub trait LearningRateScheduler: Send + Sync {
    // lr calculation (especially cosine) is sensitive to fp accuracy
    fn get_lr(&self, step: u32) -> f64;
}

#[derive(
    AnchorSerialize,
    Default,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct ConstantLR {
    base_lr: f64,
    warmup_steps: u32,
    warmup_init_lr: f64,
}

impl ConstantLR {
    #[allow(dead_code)]
    pub fn new(base_lr: f64, warmup_steps: u32, warmup_init_lr: f64) -> Self {
        ConstantLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for ConstantLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            self.base_lr
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct LinearLR {
    base_lr: f64,
    warmup_steps: u32,
    warmup_init_lr: f64,
    total_steps: u32,
    final_lr: f64,
}

impl LinearLR {
    #[allow(dead_code)]
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        total_steps: u32,
        final_lr: f64,
    ) -> Self {
        LinearLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            total_steps,
            final_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for LinearLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            self.base_lr
                + (self.final_lr - self.base_lr)
                    * ((step - self.warmup_steps) as f64
                        / (self.total_steps - self.warmup_steps) as f64)
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub struct CosineLR {
    base_lr: f64,
    warmup_steps: u32,
    warmup_init_lr: f64,
    total_steps: u32,
    final_lr: f64,
}

impl CosineLR {
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        total_steps: u32,
        final_lr: f64,
    ) -> Self {
        CosineLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            total_steps,
            final_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for CosineLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            let progress =
                (step - self.warmup_steps) as f64 / (self.total_steps - self.warmup_steps) as f64;
            let cosine_decay = 0.5 * (1.0 + (PI * progress).cos());
            self.final_lr + (self.base_lr - self.final_lr) * cosine_decay
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum LearningRateSchedule {
    Constant(ConstantLR),
    Linear(LinearLR),
    Cosine(CosineLR),
}

impl LearningRateSchedule {
    pub fn get_lr(&self, step: u32) -> f64 {
        match self {
            Self::Constant(l) => l.get_lr(step),
            Self::Linear(l) => l.get_lr(step),
            Self::Cosine(l) => l.get_lr(step),
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        match self {
            Self::Constant(l) => l.get_warmup_steps(),
            Self::Linear(l) => l.get_warmup_steps(),
            Self::Cosine(l) => l.get_warmup_steps(),
        }
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        match self {
            Self::Constant(l) => l.get_warmup_init_lr(),
            Self::Linear(l) => l.get_warmup_init_lr(),
            Self::Cosine(l) => l.get_warmup_init_lr(),
        }
    }
}

impl From<CosineLR> for LearningRateSchedule {
    fn from(value: CosineLR) -> Self {
        Self::Cosine(value)
    }
}

impl From<LinearLR> for LearningRateSchedule {
    fn from(value: LinearLR) -> Self {
        Self::Linear(value)
    }
}

impl From<ConstantLR> for LearningRateSchedule {
    fn from(value: ConstantLR) -> Self {
        Self::Constant(value)
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum OptimizerDefinition {
    Dummy,
    AdamW {
        betas: [f32; 2],
        weight_decay: f32,
        eps: f32,
        clip_grad_norm: Option<f32>,
    },
    Distro {
        clip_grad_norm: Option<f32>,
        compression_decay: f32,
        compression_decay_warmup_steps: u32,
        compression_topk: u16,
        compression_topk_startup: u16,
        compression_topk_startup_steps: u32,
        compression_chunk: u16,
        quantize_1bit: bool,
    },
}
