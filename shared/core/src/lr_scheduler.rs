use std::f64::consts::PI;

pub trait LearningRateScheduler: Send + Sync {
    fn get_lr(&self, step: u32) -> f64;
}

#[derive(Clone)]
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

#[derive(Clone)]
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

#[derive(Clone)]
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
