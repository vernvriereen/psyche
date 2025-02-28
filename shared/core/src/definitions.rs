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
        } else if step < self.total_steps {
            self.base_lr
                + (self.final_lr - self.base_lr)
                    * ((step - self.warmup_steps) as f64
                        / (self.total_steps - self.warmup_steps) as f64)
        } else {
            self.final_lr
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
pub struct WarmupStableDecayLR {
    base_lr: f64,
    warmup_steps: u32,
    warmup_init_lr: f64,
    stable_steps: u32,
    total_steps: u32,
    final_lr: f64,
}

impl WarmupStableDecayLR {
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        stable_steps: u32,
        total_steps: u32,
        final_lr: f64,
    ) -> Self {
        WarmupStableDecayLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            stable_steps,
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

impl LearningRateScheduler for WarmupStableDecayLR {
    fn get_lr(&self, step: u32) -> f64 {
        assert!(self.final_lr <= self.base_lr);
        assert!(self.stable_steps + self.warmup_steps <= self.total_steps);
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else if step < self.stable_steps + self.warmup_steps {
            self.base_lr
        } else {
            let decay_duration = self.total_steps - self.stable_steps - self.warmup_steps;
            if decay_duration > 0 {
                let steps_into_decay = step - self.stable_steps - self.warmup_steps;
                let progress = steps_into_decay as f64 / decay_duration as f64;
                let cosine_decay = 0.5 * (1.0 + (PI * progress).cos());
                self.final_lr + (self.base_lr - self.final_lr) * cosine_decay
            } else {
                self.final_lr
            }
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
    WarmupStableDecay(WarmupStableDecayLR),
}

impl LearningRateSchedule {
    pub fn get_lr(&self, step: u32) -> f64 {
        match self {
            Self::Constant(l) => l.get_lr(step),
            Self::Linear(l) => l.get_lr(step),
            Self::Cosine(l) => l.get_lr(step),
            Self::WarmupStableDecay(l) => l.get_lr(step),
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        match self {
            Self::Constant(l) => l.get_warmup_steps(),
            Self::Linear(l) => l.get_warmup_steps(),
            Self::Cosine(l) => l.get_warmup_steps(),
            Self::WarmupStableDecay(l) => l.get_warmup_steps(),
        }
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        match self {
            Self::Constant(l) => l.get_warmup_init_lr(),
            Self::Linear(l) => l.get_warmup_init_lr(),
            Self::Cosine(l) => l.get_warmup_init_lr(),
            Self::WarmupStableDecay(l) => l.get_warmup_init_lr(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_constant_lr() {
        let scheduler = ConstantLR::new(0.01, 10, 0.001);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);
        assert_relative_eq!(scheduler.get_lr(9), 0.001 + (0.01 - 0.001) * 0.9);

        // constant phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(100), 0.01);
        assert_relative_eq!(scheduler.get_lr(1000), 0.01);
    }

    #[test]
    fn test_linear_lr() {
        let scheduler = LinearLR::new(0.01, 10, 0.001, 100, 0.0001);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // linear decay phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(55), 0.01 + (0.0001 - 0.01) * 0.5);
        assert_relative_eq!(scheduler.get_lr(100), 0.0001);

        // out of bounds (should clamp to final_lr)
        assert_relative_eq!(scheduler.get_lr(200), 0.0001);
    }

    #[test]
    fn test_cosine_lr() {
        let scheduler = CosineLR::new(0.01, 10, 0.001, 110, 0.0);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // cosine decay phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(60), 0.005); // 50% of cosine cycle
        assert_relative_eq!(scheduler.get_lr(110), 0.0);
    }

    #[test]
    fn test_warmup_stable_decay_lr() {
        let scheduler = WarmupStableDecayLR::new(0.01, 10, 0.001, 60, 110, 0.0);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // stable phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(30), 0.01);
        assert_relative_eq!(scheduler.get_lr(60), 0.01);

        // cosine decay phase (after stable phase)
        let midpoint = (110 + 70) / 2; // progress from step 70 to 110
        assert_relative_eq!(scheduler.get_lr(midpoint), 0.005);
        assert_relative_eq!(scheduler.get_lr(110), 0.0);
    }

    #[test]
    fn test_edge_cases() {
        // zero warmup steps
        let scheduler = ConstantLR::new(0.01, 0, 0.001);
        assert_relative_eq!(scheduler.get_lr(0), 0.01);

        // equal initial and base LR
        let scheduler = LinearLR::new(0.01, 10, 0.01, 100, 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.01);

        // equal base and final LR
        let scheduler = CosineLR::new(0.01, 10, 0.001, 100, 0.01);
        assert_relative_eq!(scheduler.get_lr(50), 0.01);

        // zero-step schedule (edge case)
        let scheduler = WarmupStableDecayLR::new(0.01, 0, 0.001, 0, 0, 0.001);
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
    }
}
