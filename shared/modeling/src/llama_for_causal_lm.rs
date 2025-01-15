use crate::{
    llama::{Cache, Config, Llama, Llama3RopeConfig, LlamaEosToks},
    safetensor_utils::load_safetensors_into_variables,
    tensor_parallelism::Communicator,
    CausalLM, CommunicatorId, ConcreteCausalLM, LoadSafetensorsError,
};
use std::{collections::HashMap, io, path::PathBuf, sync::Arc};
use std::{collections::HashSet, rc::Rc};
use tch::{
    nn::{self, Module, VarStore},
    Device, Kind, Tensor,
};

#[cfg(feature = "parallelism")]
use tch::CNCCL;
use thiserror::Error;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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
    pub tie_word_embeddings: bool,
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

impl From<Config> for LlamaConfig {
    fn from(value: Config) -> Self {
        Self {
            hidden_size: value.hidden_size,
            intermediate_size: value.intermediate_size,
            vocab_size: value.vocab_size,
            num_hidden_layers: value.num_hidden_layers,
            num_attention_heads: value.num_attention_heads,
            num_key_value_heads: Some(value.num_key_value_heads),
            rms_norm_eps: value.rms_norm_eps,
            rope_theta: value.rope_theta,
            bos_token_id: value.bos_token_id,
            eos_token_id: value.eos_token_id,
            rope_scaling: value.rope_scaling,
            max_position_embeddings: value.max_position_embeddings,
            tie_word_embeddings: false,
        }
    }
}

fn default_rope() -> f32 {
    10_000.0
}

#[derive(Debug)]
pub struct LlamaForCausalLM {
    pub model: Llama,
    pub config: Config,
    pub variables: VarStore,
    pub device: Device,
    pub lm_head: nn::Linear,
    pub cache: Cache,
    pub comm: Option<Arc<Communicator>>,
}

#[derive(Debug, Error)]
pub enum LoadLlamaForCausalLMError {
    #[error("missing config.json")]
    MissingConfigJSON,

    #[error("failed to read file config.json")]
    FailedToReadConfig(#[from] io::Error),

    #[error("could not parse config.json")]
    FailedToParseConfig(#[from] serde_json::Error),

    #[error("this model uses tied embeddings, which aren't supported.")]
    ModelHasTiedEmbeddings,

    #[error(
        "Directly setting attention implementation to FlashAttention-2 is unsupported for now"
    )]
    ModelExplicitlyUsesFA2,

    #[error("Failed to initialize CNCCL for tensor parallelism {0}")]
    TensorParallelismFailedInit(tch::TchError),

    #[error("Tried to use tensor parallelism with feature \"parallelism\" disabled")]
    TensorParallelismNotEnabled,

    #[error("Failed to load safetensors from disk: {0}")]
    LoadSafetensorsError(#[from] LoadSafetensorsError),

    #[error("Failed to copy tensor into variable store: {0}")]
    CopyTensorError(#[from] tch::TchError),

    #[error("Some parameters were not loaded")]
    LoadTensorError,
}

impl LlamaForCausalLM {
    pub fn from_pretrained(
        repo_files: &[PathBuf],
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(Arc<CommunicatorId>, usize, usize)>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, LoadLlamaForCausalLMError> {
        let config_file = std::fs::read_to_string(
            repo_files
                .iter()
                .find(|x| x.ends_with("config.json"))
                .ok_or(LoadLlamaForCausalLMError::MissingConfigJSON)?
                .as_path(),
        )?;
        let llama_config: LlamaConfig = serde_json::from_str(&config_file)?;

        if llama_config.tie_word_embeddings {
            return Err(LoadLlamaForCausalLMError::ModelHasTiedEmbeddings);
        }

        let mut config: Config = llama_config.into_config(
            match attn_implementation.unwrap_or(AttentionImplementation::Sdpa) {
                AttentionImplementation::Eager => false,
                AttentionImplementation::Sdpa => true,
                AttentionImplementation::FlashAttention2 => {
                    return Err(LoadLlamaForCausalLMError::ModelExplicitlyUsesFA2)
                }
            },
        );

        if let Some(override_max_position_embeddings) = override_max_position_embeddings {
            config.max_position_embeddings = override_max_position_embeddings;
        }

        let device = device.unwrap_or(Device::cuda_if_available());
        #[cfg(feature = "parallelism")]
        let comm = match tensor_parallelism_world {
            // TODO: CNCCL is not Sync, though it is Send.
            // since we can't safely use it on two threads at once,
            // we should either wrap it in a Mutex, or just switch to Rc if we don't need mutability.
            #[allow(clippy::arc_with_non_send_sync)]
            Some((id, rank, world_size)) => Some(Arc::new(
                CNCCL::new(id, rank as i64, world_size as i64, device)
                    .map_err(LoadLlamaForCausalLMError::TensorParallelismFailedInit)?,
            )),
            None => None,
        };

        #[cfg(not(feature = "parallelism"))]
        let comm = match tensor_parallelism_world {
            Some(_) => return Err(LoadLlamaForCausalLMError::TensorParallelismNotEnabled),
            None => None,
        };
        let mut variables: nn::VarStore = nn::VarStore::new(device);
        if let Some(kind) = kind {
            variables.set_kind(kind);
        }
        let (model, lm_head) = {
            let _no_grad = tch::no_grad_guard();
            let model = Llama::new(variables.root(), &config, comm.clone());
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
            comm,
        })
    }

    pub fn from_p2p_shared_parameters(
        config_file_str: String,
        parameters: Rc<HashMap<String, Tensor>>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        kind: Option<Kind>,
        tensor_parallelism_world: Option<(Arc<CommunicatorId>, usize, usize)>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, LoadLlamaForCausalLMError> {
        let llama_config: LlamaConfig = serde_json::from_str(&config_file_str)?;

        if llama_config.tie_word_embeddings {
            return Err(LoadLlamaForCausalLMError::ModelHasTiedEmbeddings);
        }
        let mut config: Config = llama_config.into_config(
            match attn_implementation.unwrap_or(AttentionImplementation::Sdpa) {
                AttentionImplementation::Eager => false,
                AttentionImplementation::Sdpa => true,
                AttentionImplementation::FlashAttention2 => {
                    return Err(LoadLlamaForCausalLMError::ModelExplicitlyUsesFA2)
                }
            },
        );
        if let Some(override_max_position_embeddings) = override_max_position_embeddings {
            config.max_position_embeddings = override_max_position_embeddings;
        }

        let device = device.unwrap_or(Device::cuda_if_available());
        #[cfg(feature = "parallelism")]
        let comm = match tensor_parallelism_world {
            // TODO: CNCCL is not Sync, though it is Send.
            // since we can't safely use it on two threads at once,
            // we should either wrap it in a Mutex, or just switch to Rc if we don't need mutability.
            #[allow(clippy::arc_with_non_send_sync)]
            Some((id, rank, world_size)) => Some(Arc::new(
                CNCCL::new(id, rank as i64, world_size as i64, device)
                    .map_err(LoadLlamaForCausalLMError::TensorParallelismFailedInit)?,
            )),
            None => None,
        };
        #[cfg(not(feature = "parallelism"))]
        let comm = match tensor_parallelism_world {
            Some(_) => return Err(LoadLlamaForCausalLMError::TensorParallelismNotEnabled),
            None => None,
        };

        let mut variables: nn::VarStore = nn::VarStore::new(device);
        variables.set_kind(Kind::BFloat16);
        if let Some(kind) = kind {
            variables.set_kind(kind);
        }

        let model = Llama::new(variables.root(), &config, comm.clone());
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

        let mut unmatched = variables
            .variables()
            .keys()
            .cloned()
            .collect::<HashSet<_>>();

        {
            let _no_grad = tch::no_grad_guard();
            let mut variables = variables.variables_.lock().unwrap();
            for (name, var) in variables.named_variables.iter_mut() {
                let tensor = parameters.get(name).unwrap();
                var.f_copy_(tensor)?;
                unmatched.remove(name);
            }
            if !unmatched.is_empty() {
                return Err(LoadLlamaForCausalLMError::LoadTensorError);
            };
        }

        let cache = Cache::new(Kind::BFloat16, &config, &device);

        Ok(LlamaForCausalLM {
            model,
            config,
            variables,
            device,
            lm_head,
            cache,
            comm,
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
        let (_, t) = x.size2().unwrap();
        let mut x = self.model.forward(x, 0, &mut self.cache);
        if let Some(num_logits_to_keep) = num_logits_to_keep {
            // Only compute necessary logits, and do not upcast them to float if we are not computing the loss
            x = x.slice(1, t - num_logits_to_keep, t, 1);
        }
        let mut logits = self.lm_head.forward(&x);
        let loss = match labels {
            Some(labels) => {
                // Upcast to float if we need to compute the loss to avoid potential precision issues
                logits = logits.to_kind(Kind::Float);
                // Shift so that tokens < n predict n
                let shift_logits = logits.slice(1, 0, -1, 1).contiguous();
                let shift_labels = labels.slice(1, 1, None, 1).contiguous();
                let shift_logits = shift_logits.view([-1i64, self.config.vocab_size as i64]);
                let shift_targets = shift_labels.view(-1).to_kind(Kind::Int64);
                let loss = shift_logits.cross_entropy_loss::<Tensor>(
                    &shift_targets,
                    None,
                    tch::Reduction::Mean,
                    -100,
                    0.0,
                );
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

impl ConcreteCausalLM for LlamaForCausalLM {
    fn variables(&self) -> &VarStore {
        &self.variables
    }

    fn communicator(&self) -> Option<Arc<Communicator>> {
        self.comm.clone()
    }
}

// this is absolutely unsafe, if you use it across threads with NCCL you will have a bad day
unsafe impl Send for LlamaForCausalLM {}
