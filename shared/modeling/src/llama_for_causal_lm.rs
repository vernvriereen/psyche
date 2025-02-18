use crate::{
    tensor_parallelism::Communicator, AttentionImplementation, AutoConfig, CausalLM,
    ColumnParallelLinear, CommunicatorId, ConcreteCausalLM, ModelConfig, ModelLoadError,
    PretrainedSource, RowParallelLinear,
};
use std::{f32::consts::PI, sync::Arc};
use tch::{
    nn::{self, Module, VarStore},
    Device, Kind, Tensor,
};

#[cfg(feature = "parallelism")]
use tch::CNCCL;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub enum LlamaRopeType {
    #[serde(rename = "llama3")]
    Llama3,
    #[default]
    #[serde(rename = "default")]
    Default,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct LlamaRopeConfig {
    pub factor: f32,
    pub low_freq_factor: f32,
    pub high_freq_factor: f32,
    pub original_max_position_embeddings: usize,
    pub rope_type: LlamaRopeType,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum LlamaEosToks {
    Single(u32),
    Multiple(Vec<u32>),
}

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
    pub rope_scaling: Option<LlamaRopeConfig>,
    pub max_position_embeddings: usize,
    pub tie_word_embeddings: bool,
}

impl LlamaConfig {
    pub fn num_key_value_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }

    pub fn dummy() -> Self {
        Self {
            hidden_size: 1,
            intermediate_size: 1,
            vocab_size: 1,
            num_hidden_layers: 1,
            num_attention_heads: 1,
            num_key_value_heads: Some(1),
            rms_norm_eps: 0.00001,
            rope_theta: 10000.0,
            bos_token_id: Some(1),
            eos_token_id: Some(crate::LlamaEosToks::Single(1)),
            rope_scaling: None,
            max_position_embeddings: 2048,
            tie_word_embeddings: false,
        }
    }
}

fn default_rope() -> f32 {
    10_000.0
}

#[derive(Debug)]
pub struct Cache {
    cos: Tensor,
    sin: Tensor,
}

fn calculate_default_inv_freq(cfg: &LlamaConfig) -> Vec<f32> {
    let head_dim = cfg.hidden_size / cfg.num_attention_heads;
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / cfg.rope_theta.powf(i as f32 / head_dim as f32))
        .collect()
}

impl Cache {
    pub fn new(kind: Kind, config: &LlamaConfig, device: &Device) -> Self {
        let theta = match &config.rope_scaling {
            None
            | Some(LlamaRopeConfig {
                rope_type: LlamaRopeType::Default,
                ..
            }) => calculate_default_inv_freq(config),
            Some(rope_scaling) => {
                let low_freq_wavelen = rope_scaling.original_max_position_embeddings as f32
                    / rope_scaling.low_freq_factor;
                let high_freq_wavelen = rope_scaling.original_max_position_embeddings as f32
                    / rope_scaling.high_freq_factor;

                calculate_default_inv_freq(config)
                    .into_iter()
                    .map(|freq| {
                        let wavelen = 2. * PI / freq;
                        if wavelen < high_freq_wavelen {
                            freq
                        } else if wavelen > low_freq_wavelen {
                            freq / rope_scaling.factor
                        } else {
                            let smooth = (rope_scaling.original_max_position_embeddings as f32
                                / wavelen
                                - rope_scaling.low_freq_factor)
                                / (rope_scaling.high_freq_factor - rope_scaling.low_freq_factor);
                            (1. - smooth) * freq / rope_scaling.factor + smooth * freq
                        }
                    })
                    .collect::<Vec<_>>()
            }
        };

        let theta = Tensor::from_slice(&theta).to(*device);

        let idx_theta = Tensor::arange(
            (config.max_position_embeddings + 1) as i64,
            (Kind::Float, *device),
        )
        .reshape([(config.max_position_embeddings + 1) as i64, 1])
        .matmul(&theta.reshape([1i64, theta.numel() as i64]));
        // This is different from the paper, see:
        // https://github.com/huggingface/transformers/blob/6112b1c6442aaf7affd2b0676a1cd4eee30c45cf/src/transformers/models/llama/modeling_llama.py#L112
        let cos = idx_theta.cos().to_kind(kind);
        let sin = idx_theta.sin().to_kind(kind);
        Self { cos, sin }
    }
}

fn repeat_kv(hidden_states: &Tensor, n_rep: i64) -> Tensor {
    let (batch, num_key_value_heads, slen, head_dim) = hidden_states.size4().unwrap();

    if n_rep == 1 {
        return hidden_states.shallow_clone();
    }

    let hidden_states = hidden_states
        .unsqueeze(2)
        .expand([batch, num_key_value_heads, n_rep, slen, head_dim], false);

    hidden_states.reshape([batch, num_key_value_heads * n_rep, slen, head_dim])
}

fn rotate_half(xs: &Tensor) -> Tensor {
    let last_dim = *xs.size().last().unwrap();
    let xs1 = xs.narrow(-1, 0, last_dim / 2);
    let xs2 = xs.narrow(-1, last_dim / 2, last_dim - last_dim / 2);
    Tensor::cat(&[&xs2.neg(), &xs1], -1)
}

#[derive(Debug)]
struct RmsNorm {
    weight: Tensor,
    eps: f64,
}

impl RmsNorm {
    fn new(vs: nn::Path, size: i64, eps: f64) -> Self {
        let weight = vs.ones("weight", &[size]);
        Self { weight, eps }
    }
}

impl Module for RmsNorm {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let kind = xs.kind();
        let xs = xs.to_kind(Kind::Float);
        let variance = xs.pow_tensor_scalar(2).mean_dim(-1, true, Kind::Float);
        let xs_normed = xs * (variance + self.eps).rsqrt();
        let xs_normed = xs_normed.to_kind(kind);
        &self.weight * xs_normed
    }
}

#[derive(Debug)]
struct Mlp {
    gate_proj: ColumnParallelLinear,
    up_proj: ColumnParallelLinear,
    down_proj: RowParallelLinear,
}

impl Mlp {
    fn new(vs: nn::Path, n_embd: i64, n_hidden: i64, comm: Option<Arc<Communicator>>) -> Self {
        let tp_size = comm.as_ref().map(|x| x.size()).unwrap_or(1);
        assert_eq!(
            n_hidden % tp_size,
            0,
            "n_hidden must be divisible by tp_size"
        );

        let gate_proj = ColumnParallelLinear::new(
            &vs / "gate_proj",
            n_embd,
            n_hidden,
            false,
            false,
            comm.clone(),
        );
        let up_proj = ColumnParallelLinear::new(
            &vs / "up_proj",
            n_embd,
            n_hidden,
            false,
            false,
            comm.clone(),
        );
        let down_proj =
            RowParallelLinear::new(&vs / "down_proj", n_hidden, n_embd, false, true, comm);
        Self {
            gate_proj,
            up_proj,
            down_proj,
        }
    }
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Tensor {
        self.down_proj
            .forward(&(self.gate_proj.forward(xs).silu() * self.up_proj.forward(xs)))
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct CausalSelfAttention {
    q_proj: ColumnParallelLinear,
    k_proj: ColumnParallelLinear,
    v_proj: ColumnParallelLinear,
    o_proj: RowParallelLinear,
    n_head: i64,
    n_kvhead: i64,
    n_embd: i64,
    n_max_seq_len: i64,
    head_dim: i64,
    device: Device,
    use_sdpa: bool,
    tp_size: i64,
}

impl CausalSelfAttention {
    fn new(
        vs: nn::Path,
        n_head: i64,
        n_kvheads: i64,
        n_embd: i64,
        n_max_seq_len: i64,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let tp_size = comm.as_ref().map(|x| x.size()).unwrap_or(1);
        assert_eq!(n_head % tp_size, 0, "n_head must be divisible by tp_size");
        assert_eq!(
            n_kvheads % tp_size,
            0,
            "n_kvheads must be divisible by tp_size"
        );

        let head_dim = n_embd / n_head;
        let size_q = head_dim * n_head;
        let size_kv = head_dim * n_kvheads;

        let q_proj =
            ColumnParallelLinear::new(&vs / "q_proj", n_embd, size_q, false, false, comm.clone());
        let k_proj =
            ColumnParallelLinear::new(&vs / "k_proj", n_embd, size_kv, false, false, comm.clone());
        let v_proj =
            ColumnParallelLinear::new(&vs / "v_proj", n_embd, size_kv, false, false, comm.clone());
        let o_proj = RowParallelLinear::new(&vs / "o_proj", size_q, n_embd, false, true, comm);

        Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            n_kvhead: n_kvheads,
            n_embd,
            n_max_seq_len,
            head_dim,
            device: vs.device(),
            use_sdpa,
            tp_size,
        }
    }

    fn apply_rotary_emb(&self, x: &Tensor, index_pos: i64, cache: &Cache) -> Tensor {
        let (_b_sz, _, seq_len, _hidden_size) = x.size4().unwrap();
        let cos = cache.cos.narrow(0, index_pos, seq_len);
        let sin = cache.sin.narrow(0, index_pos, seq_len);
        let cos = Tensor::cat(&[&cos, &cos], -1);
        let sin = Tensor::cat(&[&sin, &sin], -1);
        let cos = cos.unsqueeze(0).unsqueeze(0);
        let sin = sin.unsqueeze(0).unsqueeze(0);
        (x * cos) + (rotate_half(x) * sin)
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut Cache) -> Tensor {
        let (b, t, c) = x.size3().unwrap();
        assert_eq!(c, self.n_embd, "Input hidden size mismatch");
        let kind = x.kind();

        let q = self.q_proj.forward(x);
        let k = self.k_proj.forward(x);
        let v = self.v_proj.forward(x);

        let local_n_head = self.n_head / self.tp_size;
        let local_n_kvhead = self.n_kvhead / self.tp_size;

        let q = q
            .contiguous()
            .reshape([b, t, local_n_head, self.head_dim])
            .transpose(1, 2);
        let k = k
            .contiguous()
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);
        let v = v
            .contiguous()
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);

        let q = self.apply_rotary_emb(&q, index_pos, cache).to_kind(kind);
        let k = self.apply_rotary_emb(&k, index_pos, cache).to_kind(kind);

        let k = repeat_kv(&k, local_n_head / local_n_kvhead);
        let v = repeat_kv(&v, local_n_head / local_n_kvhead);

        let scale = 1.0 / (self.head_dim as f64).sqrt();

        let y = if self.use_sdpa {
            let att = Tensor::scaled_dot_product_attention::<Tensor>(
                &q,
                &k,
                &v,
                None,
                0.0,
                t > 1,
                Some(scale),
            );
            att.transpose(1, 2)
                .contiguous()
                .reshape([b, t, local_n_head * self.head_dim])
        } else {
            let att = q.matmul(&k.transpose(-2, -1)) * scale;
            let mask = Tensor::ones([t, t], (kind, self.device))
                .tril(0)
                .reshape([1, 1, t, t]);
            let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
            let y = att.softmax(-1, kind).matmul(&v);
            y.transpose(1, 2)
                .contiguous()
                .reshape([b, t, local_n_head * self.head_dim])
        };

        self.o_proj.forward(&y)
    }
}

#[derive(Debug)]
struct Block {
    rms_1: RmsNorm,
    attn: CausalSelfAttention,
    rms_2: RmsNorm,
    mlp: Mlp,
}

impl Block {
    fn new(
        vs: nn::Path,
        config: &LlamaConfig,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let rms_1 = RmsNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let attn = CausalSelfAttention::new(
            &vs / "self_attn",
            config.num_attention_heads as i64,
            config
                .num_key_value_heads
                .unwrap_or(config.num_attention_heads) as i64,
            config.hidden_size as i64,
            (config.max_position_embeddings + 1) as i64,
            use_sdpa,
            comm.clone(),
        );
        let rms_2 = RmsNorm::new(
            &vs / "post_attention_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let mlp = Mlp::new(
            &vs / "mlp",
            config.hidden_size as i64,
            config.intermediate_size as i64,
            comm,
        );
        Self {
            rms_1,
            attn,
            rms_2,
            mlp,
        }
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut Cache) -> Tensor {
        let x = self.attn.forward(&self.rms_1.forward(x), index_pos, cache) + x;
        self.mlp.forward(&self.rms_2.forward(&x)) + x
    }
}

#[derive(Debug)]
pub struct Llama {
    wte: nn::Embedding,
    blocks: Vec<Block>,
    ln_f: RmsNorm,
}

impl Llama {
    pub fn new(
        vs: nn::Path,
        config: &LlamaConfig,
        use_sdpa: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let wte = nn::embedding(
            &vs / "model" / "embed_tokens",
            config.vocab_size as i64,
            config.hidden_size as i64,
            Default::default(),
        );
        let ln_f = RmsNorm::new(
            &vs / "model" / "norm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let blocks = (0..config.num_hidden_layers)
            .map(|i| Block::new(&vs / "model" / "layers" / i, config, use_sdpa, comm.clone()))
            .collect::<Vec<_>>();
        Self { wte, blocks, ln_f }
    }

    pub fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut Cache) -> Tensor {
        let mut x = self.wte.forward(x);
        for block in &self.blocks {
            x = block.forward(&x, index_pos, cache);
        }
        self.ln_f.forward(&x)
    }
}

#[derive(Debug)]
pub struct LlamaForCausalLM {
    pub model: Llama,
    pub config: LlamaConfig,
    pub variables: VarStore,
    pub device: Device,
    pub lm_head: nn::Linear,
    pub cache: Cache,
    pub comm: Option<Arc<Communicator>>,
}

impl LlamaForCausalLM {
    pub fn from_pretrained(
        source: &PretrainedSource<LlamaConfig>,
        kind: Option<Kind>,
        attn_implementation: Option<AttentionImplementation>,
        device: Option<Device>,
        tensor_parallelism_world: Option<(Arc<CommunicatorId>, usize, usize)>,
        override_max_position_embeddings: Option<usize>,
    ) -> Result<Self, ModelLoadError> {
        let mut config = source.get_config()?;

        if config.tie_word_embeddings {
            return Err(ModelLoadError::ModelHasTiedEmbeddings);
        }

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
                    .map_err(ModelLoadError::TensorParallelismFailedInit)?,
            )),
            None => None,
        };

        #[cfg(not(feature = "parallelism"))]
        let comm = match tensor_parallelism_world {
            Some(_) => return Err(ModelLoadError::TensorParallelismNotEnabled),
            None => None,
        };
        let mut variables: nn::VarStore = nn::VarStore::new(device);
        if let Some(kind) = kind {
            variables.set_kind(kind);
        }
        let (model, lm_head) = {
            let _no_grad = tch::no_grad_guard();
            let model = Llama::new(
                variables.root(),
                &config,
                match attn_implementation {
                    Some(AttentionImplementation::Eager) => false,
                    Some(AttentionImplementation::FlashAttention2) => {
                        return Err(ModelLoadError::ModelExplicitlyUsesFA2);
                    }
                    Some(AttentionImplementation::Sdpa) => true,
                    None => true,
                },
                comm.clone(),
            );
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

            source.load(&mut variables)?;

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

impl ModelConfig for LlamaConfig {
    // TODO: This is just a hacky solution to get the parameter names from the config
    // but it is probably overkill. We should think about a better way to get them
    // to make the p2p requests.
    fn get_parameter_names(&self) -> Vec<String> {
        let mut variables: nn::VarStore = nn::VarStore::new(Device::Cpu);
        variables.set_kind(Kind::BFloat16);
        let _model = Llama::new(variables.root(), self, false, None);
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };

        let _lm_head = nn::linear(
            &variables.root() / "lm_head",
            self.hidden_size as i64,
            self.vocab_size as i64,
            c,
        );

        let variables_lock = variables.variables_.lock().unwrap();
        variables_lock.named_variables.keys().cloned().collect()
    }
}

impl TryFrom<AutoConfig> for LlamaConfig {
    type Error = ModelLoadError;

    fn try_from(value: AutoConfig) -> Result<Self, Self::Error> {
        match value {
            AutoConfig::Llama(llama_config) => Ok(llama_config),
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}

impl TryFrom<PretrainedSource<AutoConfig>> for PretrainedSource<LlamaConfig> {
    type Error = ModelLoadError;

    fn try_from(value: PretrainedSource<AutoConfig>) -> Result<Self, Self::Error> {
        match value {
            PretrainedSource::RepoFiles(path_bufs) => Ok(PretrainedSource::RepoFiles(path_bufs)),
            PretrainedSource::ConfigAndTensors(AutoConfig::Llama(config), hash_map) => {
                Ok(PretrainedSource::ConfigAndTensors(config, hash_map))
            }
            _ => Err(ModelLoadError::WrongConfigType),
        }
    }
}
