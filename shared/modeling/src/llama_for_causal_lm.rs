use std::{path::PathBuf, rc::Rc};

use crate::{
    llama::{Cache, Config, Llama, Llama3RopeConfig, LlamaEosToks},
    safetensor_loader::load_safetensors_into_variables,
    tensor_parallelism::AllReduce,
    CausalLM,
};
use anyhow::{bail, Error, Result};
use cudarc::{
    driver::CudaDevice,
    nccl::{Comm, Id, ReduceOp},
};
use tch::{
    nn::{self, Module, VarStore},
    Device, Kind, Tensor,
};

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

#[derive(serde::Deserialize)]
pub enum AttentionImplementation {
    #[serde(rename = "eager")]
    Eager,
    #[serde(rename = "sdpa")]
    Sdpa,
    #[serde(rename = "flash_attention_2")]
    FlashAttention2,
}

impl LlamaConfig {
    pub fn num_key_value_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }

    pub fn into_config(self, use_sdpa: bool) -> Config {
        Config {
            hidden_size: self.hidden_size,
            intermediate_size: self.intermediate_size,
            vocab_size: self.vocab_size,
            num_hidden_layers: self.num_hidden_layers,
            num_attention_heads: self.num_attention_heads,
            num_key_value_heads: self.num_key_value_heads(),
            rms_norm_eps: self.rms_norm_eps,
            rope_theta: self.rope_theta,
            bos_token_id: self.bos_token_id,
            eos_token_id: self.eos_token_id,
            rope_scaling: self.rope_scaling,
            max_position_embeddings: self.max_position_embeddings,
            use_sdpa,
        }
    }
}

fn default_rope() -> f32 {
    10_000.0
}

pub struct LlamaForCausalLM {
    pub model: Llama,
    pub config: Config,
    pub variables: VarStore,
    pub device: Device,
    lm_head: nn::Linear,
    cache: Cache,
}

impl LlamaForCausalLM {
    pub fn from_pretrained(
        repo_files: &[PathBuf],
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(Id, usize)>,
    ) -> Result<Self> {
        let llama_config: LlamaConfig = serde_json::from_str(&String::from_utf8(std::fs::read(
            repo_files
                .iter()
                .find(|x| x.ends_with("config.json"))
                .ok_or(Error::msg("missing config.json"))?
                .as_path(),
        )?)?)?;
        let config: Config = llama_config.into_config(match attn_implementation.unwrap_or(AttentionImplementation::Sdpa) {
            AttentionImplementation::Eager => false,
            AttentionImplementation::Sdpa => true,
            AttentionImplementation::FlashAttention2 => { bail!("Directly setting attention implementation to FlashAttention-2 unsupported for now"); }
        });
        let device = device.unwrap_or(Device::Cuda(0));
        let comm: Option<Rc<Comm>> = match tensor_parallelism_world {
            Some((master_id, world_size)) => {
                let rank = match device {
                    Device::Cuda(rank) => rank,
                    _ => {
                        bail!("TP requires CUDA");
                    }
                };
                let cuda_device = CudaDevice::new(rank)?;
                let comm = match Comm::from_rank(cuda_device, rank, world_size, master_id) {
                    Ok(comm) => Rc::new(comm),
                    Err(err) => {
                        bail!("nccl error: {:?}", err.0);
                    }
                };
                Some(comm)
            }
            None => None,
        };
        let mut variables: nn::VarStore = nn::VarStore::new(device);
        if let Some(kind) = kind {
            variables.set_kind(kind);
        }
        let (model, lm_head) = {
            let _no_grad = tch::no_grad_guard();
            let model = Llama::new(variables.root(), &config, comm);
            let c = nn::LinearConfig {
                bias: false,
                ..Default::default()
            };
            let lm_head = nn::linear(
                &variables.root() / "lm_head",
                config.hidden_size as i64,
                config.vocab_size as i64,
                c,
            );
            load_safetensors_into_variables(&mut variables, repo_files)?;
            (model, lm_head)
        };
        let cache = Cache::new(kind.unwrap_or(Kind::Float), &config, &device);
        Ok(LlamaForCausalLM {
            model,
            config,
            variables,
            device,
            lm_head,
            cache,
        })
    }
}

impl CausalLM for LlamaForCausalLM {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>) {
        let world_size = self.model.comm.as_ref().map(|c| c.world_size() as f64).unwrap_or(1.0);
        let (_, t) = x.size2().unwrap();
        let mut x = self.model.forward(x, 0, &mut self.cache);
        if let Some(num_logits_to_keep) = num_logits_to_keep {
            // Only compute necessary logits, and do not upcast them to float if we are not computing the loss
            x = x.slice(1, t - num_logits_to_keep, t, 1);
        }
        let logits = self.lm_head.forward(&x);

        let logits_max = logits.max_dim(-1, true).0;
        let logits_max = logits_max.all_reduce(&self.model.comm, ReduceOp::Max);
        let mut logits = (logits - logits_max).all_reduce(&self.model.comm, ReduceOp::Sum) / world_size;

        let loss = match labels {
            Some(labels) => {
                // Upcast to float if we need to compute the loss to avoid potential precision issues
                logits = logits.to_kind(Kind::Float);
                // Shift so that tokens < n predict n
                let shift_logits = logits.slice(1, 0, -1, 1).contiguous();
                let shift_labels = labels.slice(1, 1, None, 1).contiguous();
                let shift_logits = shift_logits.view([-1i64, self.config.vocab_size as i64]);
                let shift_targets = shift_labels.view(-1).to_kind(Kind::Int64);
                let loss = shift_logits.cross_entropy_for_logits(&shift_targets);
                Some(loss)
            }
            None => None,
        };
        (logits, loss)
    }

    fn bos_token_id(&self) -> Option<i64> {
        self.config.bos_token_id.map(|x| x as i64)
    }

    fn device(&self) -> Device {
        self.device
    }
}
