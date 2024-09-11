use anyhow::Result;
use psyche_core::{CosineLR, LearningRateScheduler};
use psyche_data_provider::{LocalDataProvider, TokenSize};
use psyche_training::{Batcher, LlamaForCausalLM};
use rand::Rng;
use std::time::SystemTime;
use tch::nn::{self, OptimizerConfig};
use tch::{Device, Kind, Tensor};

const TOKEN_SIZE_IN_BYTES: TokenSize = TokenSize::TwoBytes;
const MICRO_BATCH_SIZE: usize = 1;
const TOTAL_BATCH_SIZE: usize = 16;
const GRAD_ACCUM_STEPS: usize = TOTAL_BATCH_SIZE / MICRO_BATCH_SIZE;
const ADAMW: nn::AdamW = nn::AdamW {
    beta1: 0.9,
    beta2: 0.95,
    wd: 0.1,
    eps: 1e-8,
    amsgrad: false,
};
const PEAK_LEARNING_RATE: f64 = 4e-4;
const WARMUP_STEPS: usize = 500;
const TOTAL_STEPS: usize = 25000;
const MAX_GRAD_NORM: f64 = 1.0;
const REPO_ID: &str = "emozilla/llama2-1.2b-init";

fn main() -> Result<()> {
    let model = LlamaForCausalLM::from_pretrained(REPO_ID, Some(Kind::BFloat16), None, None)?;
    let device = Device::Cuda(0);
    let dataset = LocalDataProvider::new_from_directory(
        "training/data",
        TOKEN_SIZE_IN_BYTES,
        model.config.max_position_embeddings,
        rand::thread_rng().gen(),
    )?;

    let iter = dataset.into_iter().map(|tokens| {
        Ok((
            Tensor::from_slice(&tokens).to(device),
            Tensor::from_slice(&tokens).to(device),
        ))
    });
    let mut batch_iter = Batcher::new_r2(iter).batch_size(MICRO_BATCH_SIZE);
    let schedule = CosineLR::new(
        PEAK_LEARNING_RATE,
        WARMUP_STEPS,
        0.0,
        TOTAL_STEPS,
        PEAK_LEARNING_RATE / 10.0,
    );
    let mut opt = ADAMW.build(&model.variables, PEAK_LEARNING_RATE)?;
    let grad_accum_divisor: Tensor = (GRAD_ACCUM_STEPS as f32).into();
    let grad_accum_divisor = grad_accum_divisor.to(device);
    for step in 0..TOTAL_STEPS {
        let start_time = SystemTime::now();
        let lr = schedule.get_lr(step);
        opt.set_lr(lr);
        let mut avg_loss: f32 = 0.0;
        for _ in 0..GRAD_ACCUM_STEPS {
            let (inputs, targets) = batch_iter.next().unwrap()?;
            let (_, loss) = model.forward(&inputs, Some(&targets), None);
            let loss = loss.unwrap() / grad_accum_divisor.copy();
            loss.backward();
            let loss_value: f32 = loss.try_into()?;
            avg_loss += loss_value;
        }
        opt.clip_grad_norm(MAX_GRAD_NORM);
        opt.step();
        opt.zero_grad();
        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();
        println!(
            "step: {}, duration: {:.1}, lr: {:.1e}, loss: {:.4}",
            step, duration, lr, avg_loss
        );
    }
    Ok(())
}
