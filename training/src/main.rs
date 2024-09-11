use crate::llama::{Config, Llama, Llama3RopeConfig, LlamaEosToks};
use anyhow::{Error, Result};
use batcher::Batcher;
use psyche_client::download_repo_sync;
use psyche_coordinator::model::HubRepo;
use psyche_core::{CosineLR, LearningRateScheduler};
use psyche_data_provider::{make_pretraining_samples, LocalDataProvider, TokenSize};
use rand::Rng;
use std::time::SystemTime;
use tch::nn::{self, OptimizerConfig};
use tch::{Device, Tensor};

mod batcher;
mod llama;

// #[allow(dead_code)]
// const CONFIG_1_2B: Config = Config {
//     vocab_size: 32000,
//     n_layer: 16,
//     n_head: 16,
//     n_embd: 4096,
//     seq_len: 2048,
// };
// #[allow(dead_code)]
// const CONFIG_200M: Config = Config {
//     vocab_size: 32000,
//     n_layer: 12,
//     n_head: 12,
//     n_embd: 768,
//     seq_len: 2048,
// };
// #[allow(dead_code)]
// const CONFIG_100M: Config = Config {
//     vocab_size: 32000,
//     n_layer: 6,
//     n_head: 8,
//     n_embd: 512,
//     seq_len: 2048,
// };
// const CONFIG: &Config = &CONFIG_100M;
const TOKEN_SIZE_IN_BYTES: TokenSize = TokenSize::TwoBytes;
const MICRO_BATCH_SIZE: usize = 8;
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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlamaConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: Option<usize>,
    pub rms_norm_eps: f64,
    #[serde(default = "default_rope")]
    pub rope_theta: f32,
    pub bos_token_id: Option<u32>,
    pub eos_token_id: Option<LlamaEosToks>,
    pub rope_scaling: Option<Llama3RopeConfig>,
    pub max_position_embeddings: usize,
}

impl LlamaConfig {
    pub fn num_key_value_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }
}

fn default_rope() -> f32 {
    10_000.0
}

impl From<LlamaConfig> for Config {
    fn from(value: LlamaConfig) -> Self {
        Config {
            hidden_size: value.hidden_size,
            intermediate_size: value.intermediate_size,
            vocab_size: value.vocab_size,
            num_hidden_layers: value.num_hidden_layers,
            num_attention_heads: value.num_attention_heads,
            num_key_value_heads: value.num_key_value_heads(),
            rms_norm_eps: value.rms_norm_eps,
            rope_theta: value.rope_theta,
            bos_token_id: value.bos_token_id,
            eos_token_id: value.eos_token_id,
            rope_scaling: value.rope_scaling,
            max_position_embeddings: value.max_position_embeddings,
        }
    }
}

fn main() -> Result<()> {
    let repo_files = download_repo_sync(
        HubRepo {
            repo_id: REPO_ID.to_owned(),
            revision: None,
        },
        None,
        None,
        true,
    )?;
    let config: LlamaConfig = serde_json::from_str(&String::from_utf8(std::fs::read(
        repo_files
            .iter()
            .find(|x| x.ends_with("config.json"))
            .ok_or(Error::msg("missing config.json"))?
            .as_path(),
    )?)?)?;
    let device = Device::Cuda(0);
    let dataset = LocalDataProvider::new_from_directory(
        "training/data",
        TOKEN_SIZE_IN_BYTES,
        config.max_position_embeddings,
        rand::thread_rng().gen(),
    )?;
    let mut vs: nn::VarStore = nn::VarStore::new(device);
    let model = Llama::new(vs.root(), &config.clone().into());
    vs.bfloat16();
    let iter = dataset.into_iter().map(|tokens| {
        let (inputs, targets) = make_pretraining_samples(&tokens);
        Ok((
            Tensor::from_slice(inputs).to(device),
            Tensor::from_slice(targets).to(device),
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
    let mut opt = ADAMW.build(&vs, PEAK_LEARNING_RATE)?;
    let grad_accum_divisor: Tensor = (GRAD_ACCUM_STEPS as f32).into();
    let grad_accum_divisor = grad_accum_divisor.to(device);
    for step in 0..TOTAL_STEPS {
        let start_time = SystemTime::now();
        let lr = schedule.get_lr(step);
        opt.set_lr(lr);
        let mut avg_loss: f32 = 0.0;
        for _ in 0..GRAD_ACCUM_STEPS {
            let (inputs, targets) = batch_iter.next().unwrap()?;
            let logits = model.forward(&inputs);
            let shift_logits = logits.view([-1i64, config.vocab_size as i64]);
            let shift_targets = targets.view(-1).to_kind(tch::Kind::Int64);
            let loss =
                shift_logits.cross_entropy_for_logits(&shift_targets) / grad_accum_divisor.copy();
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
