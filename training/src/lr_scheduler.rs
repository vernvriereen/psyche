use std::f64::consts::PI;

pub trait LearningRateScheduler {
    fn get_lr(&self, step: usize) -> f64;
}

pub struct ConstantLR {
    base_lr: f64,
    warmup_steps: usize,
    warmup_init_lr: f64,
}

impl ConstantLR {
    #[allow(dead_code)]
    pub fn new(base_lr: f64, warmup_steps: usize, warmup_init_lr: f64) -> Self {
        ConstantLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
        }
    }
}

impl LearningRateScheduler for ConstantLR {
    fn get_lr(&self, step: usize) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            self.base_lr
        }
    }
}

pub struct LinearLR {
    base_lr: f64,
    warmup_steps: usize,
    warmup_init_lr: f64,
    total_steps: usize,
    final_lr: f64,
}

impl LinearLR {
    #[allow(dead_code)]
    pub fn new(
        base_lr: f64,
        warmup_steps: usize,
        warmup_init_lr: f64,
        total_steps: usize,
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
    fn get_lr(&self, step: usize) -> f64 {
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

pub struct CosineLR {
    base_lr: f64,
    warmup_steps: usize,
    warmup_init_lr: f64,
    total_steps: usize,
    final_lr: f64,
}

impl CosineLR {
    pub fn new(
        base_lr: f64,
        warmup_steps: usize,
        warmup_init_lr: f64,
        total_steps: usize,
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
    fn get_lr(&self, step: usize) -> f64 {
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
