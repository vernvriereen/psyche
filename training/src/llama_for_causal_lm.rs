use crate::{
    llama::{Config, Llama, Llama3RopeConfig, LlamaEosToks},
    safetensor_loader::load_safetensors_into_variables,
};
use anyhow::{bail, Error, Result};
use psyche_client::download_repo_sync;
use tch::{
    nn::{self, Module, VarStore},
    Device, Kind, Tensor,
};
// use tokenizers::Tokenizer;

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
    SDPA,
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
    // pub tokenizer: Option<Tokenizer>,
    lm_head: nn::Linear,
}

impl LlamaForCausalLM {
    pub fn from_pretrained(
        repo_id: &str,
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
    ) -> Result<Self> {
        let repo_files = download_repo_sync(repo_id.to_owned(), None, None, None, true)?;
        let llama_config: LlamaConfig = serde_json::from_str(&String::from_utf8(std::fs::read(
            repo_files
                .iter()
                .find(|x| x.ends_with("config.json"))
                .ok_or(Error::msg("missing config.json"))?
                .as_path(),
        )?)?)?;
        let config: Config = llama_config.into_config(match attn_implementation.unwrap_or(AttentionImplementation::SDPA) {
            AttentionImplementation::Eager => false,
            AttentionImplementation::SDPA => true,
            AttentionImplementation::FlashAttention2 => { bail!("Directly setting attention implementation to FlashAttention-2 unsupported for now"); }
        });
        let mut variables: nn::VarStore = nn::VarStore::new(device.unwrap_or(Device::Cuda(0)));
        let (model, lm_head) = {
            let _no_grad = tch::no_grad_guard();
            let model = Llama::new(variables.root(), &config);
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
            match kind {
                Some(Kind::BFloat16) => variables.bfloat16(),
                Some(Kind::Float) => variables.float(),
                Some(Kind::Half) => variables.half(),
                _ => {}
            };
            load_safetensors_into_variables(&mut variables, &repo_files)?;
            (model, lm_head)
        };
        // let tokenizer = match repo_files.iter().find(|x| x.ends_with("tokenizer.json")) {
        //     Some(path) => Some(Tokenizer::from_file(path.as_path()).map_err(Error::msg)?),
        //     None => None,
        // };
        Ok(LlamaForCausalLM {
            model,
            config,
            variables,
            // tokenizer,
            lm_head,
        })
    }

    pub fn forward(
        &self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>) {
        let (_, t) = x.size2().unwrap();
        let mut x = self.model.forward(x);
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
                let shift_logits = logits.slice(1, 0, -1, 1);
                let shift_labels = labels.slice(1, 1, None, 1);
                let shift_logits = shift_logits.view([-1i64, self.config.vocab_size as i64]);
                let shift_targets = shift_labels.view(-1).to_kind(Kind::Int64);
                Some(shift_logits.cross_entropy_for_logits(&shift_targets))
            }
            None => None,
        };
        (logits, loss)
    }
}
