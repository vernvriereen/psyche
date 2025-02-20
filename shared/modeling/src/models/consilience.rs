use crate::{default_rope, CausalSelfAttention, ColumnParallelLinear, Communicator, LlamaEosToks, RoPECache, RoPEConfig, RowParallelLinear};

use std::{f32::consts::PI, sync::Arc};
use tch::nn::{self, Module};
use tch::{Device, Kind, Tensor};
use torch_sys::IntList;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ConsilienceConfig {
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
    pub rope_scaling: Option<RoPEConfig>,
    pub max_position_embeddings: usize,
    pub tie_word_embeddings: bool,
    pub q_lora_rank: Option<usize>,
    pub kv_lora_rank: Option<usize>,
    pub qk_nope_head_dim: Option<usize>,
    pub qk_rope_head_dim: Option<usize>,
    pub v_head_dim: Option<usize>,
    pub attention_bias: bool,
}

impl ConsilienceConfig {
    pub fn num_key_value_heads(&self) -> usize {
        self.num_key_value_heads.unwrap_or(self.num_attention_heads)
    }
}


impl std::fmt::Display for ConsilienceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_string(self)
            .map_err(|_| std::fmt::Error)
            .and_then(|s| write!(f, "{}", s))
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

#[derive(Debug)]
struct MLA {
    q_a_proj: ColumnParallelLinear,
    q_a_layernorm: RmsNorm,
    q_b_proj: ColumnParallelLinear,

    kv_a_proj_with_mqa: ColumnParallelLinear,
    kv_a_layernorm: RmsNorm,
    kv_b_proj: ColumnParallelLinear,

    o_proj: RowParallelLinear,

    num_heads: i64,
    head_v_dim: i64,
    qk_rope_head_dim: i64,
    qk_nope_head_dim: i64,
    softmax_scale: f64,
    device: Device,
    use_sdpa: bool,
    kv_lora_rank: i64,
}

impl MLA {
    fn new(vs: nn::Path, config: &ConsilienceConfig, use_sdpa: bool, comm: Option<Arc<Communicator>>) -> Self {
        let hidden_size = config.hidden_size as i64;
        let num_heads = config.num_attention_heads as i64;
        let qk_rope_head_dim = config.qk_rope_head_dim.unwrap() as i64;
        let qk_nope_head_dim = config.qk_nope_head_dim.unwrap() as i64;
        let v_head_dim = config.v_head_dim.unwrap() as i64;
        let q_head_dim = qk_nope_head_dim + qk_rope_head_dim;
        let kv_lora_rank = config.kv_lora_rank.unwrap() as i64;

        let q_a_proj = ColumnParallelLinear::new(
            &vs / "q_a_proj",
            hidden_size,
            config.q_lora_rank.unwrap() as i64,
            config.attention_bias,
            false,
            comm.clone(),
        );

        let q_a_layernorm = RmsNorm::new(
            &vs / "q_a_layernorm",
            config.q_lora_rank.unwrap() as i64,
            config.rms_norm_eps,
        );

        let q_b_proj = ColumnParallelLinear::new(
            &vs / "q_b_proj",
            config.q_lora_rank.unwrap() as i64,
            num_heads * q_head_dim,
            false,
            false,
            comm.clone(),
        );

        let kv_a_proj_with_mqa = ColumnParallelLinear::new(
            &vs / "kv_a_proj_with_mqa",
            hidden_size,
            config.kv_lora_rank.unwrap() as i64 + qk_rope_head_dim,
            config.attention_bias,
            false,
            comm.clone(),
        );

        let kv_a_layernorm = RmsNorm::new(
            &vs / "kv_a_layernorm",
            config.kv_lora_rank.unwrap() as i64,
            config.rms_norm_eps,
        );

        let kv_b_proj = ColumnParallelLinear::new(
            &vs / "kv_b_proj",
            config.kv_lora_rank.unwrap() as i64,
            num_heads * (q_head_dim - qk_rope_head_dim + v_head_dim),
            false,
            false,
            comm.clone(),
        );

        let o_proj = RowParallelLinear::new(
            &vs / "o_proj",
            num_heads * v_head_dim,
            hidden_size,
            config.attention_bias,
            true,
            comm,
        );

        let softmax_scale = 1.0 / (q_head_dim as f64).sqrt();

        Self {
            q_a_proj,
            q_a_layernorm,
            q_b_proj,
            kv_a_proj_with_mqa,
            kv_a_layernorm,
            kv_b_proj,
            o_proj,
            num_heads,
            head_v_dim: v_head_dim,
            qk_rope_head_dim,
            qk_nope_head_dim,
            softmax_scale,
            device: vs.device(),
            use_sdpa: use_sdpa,
            kv_lora_rank,
        }
    }

    fn split_with_sizes_2(tensor: Tensor, split_sizes: impl IntList, dim: i64) -> (Tensor, Tensor) {
        let mut tensors = tensor.split_with_sizes(split_sizes, dim);
        let b = tensors.pop().unwrap();
        let a = tensors.pop().unwrap();
        (a, b)
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut RoPECache) -> Tensor {
        let (b, t, _) = x.size3().unwrap();
        let kind = x.kind();

        let q_compressed = self.q_a_proj.forward(x);
        let q_compressed = self.q_a_layernorm.forward(&q_compressed);
        let q = self.q_b_proj.forward(&q_compressed);
        let q = q.view([b, t, self.num_heads, -1]).transpose(1, 2);
        let (q_nope, q_pe) =
            Self::split_with_sizes_2(q, &[self.qk_nope_head_dim, self.qk_rope_head_dim], -1);

        let compressed_kv = self.kv_a_proj_with_mqa.forward(x);
        let (compressed_kv, k_pe) = Self::split_with_sizes_2(
            compressed_kv,
            &[self.kv_lora_rank as i64, self.qk_rope_head_dim],
            -1,
        );
        let k_pe = k_pe.view([b, t, 1, self.qk_rope_head_dim]).transpose(1, 2);

        let compressed_kv = self.kv_a_layernorm.forward(&compressed_kv);
        let kv = self
            .kv_b_proj
            .forward(&compressed_kv)
            .view([
                b,
                t,
                self.num_heads,
                self.qk_nope_head_dim + self.head_v_dim,
            ])
            .transpose(1, 2);

        let (k_nope, value_states) =
            Self::split_with_sizes_2(kv, &[self.qk_nope_head_dim, self.head_v_dim], -1);

        let q_pe = cache.apply_rotary_emb(&q_pe, index_pos).to_kind(kind);
        let k_pe = cache.apply_rotary_emb(&k_pe, index_pos).to_kind(kind);

        let query_states = Tensor::cat(&[&q_nope, &q_pe], -1);
        let key_states = Tensor::cat(&[&k_nope, &k_pe], -1);

        let y = if self.use_sdpa {
            Tensor::scaled_dot_product_attention::<Tensor>(
                &query_states,
                &key_states,
                &value_states,
                None,
                0.0,
                t > 1,
                Some(self.softmax_scale),
            )
        } else {
            let att = query_states.matmul(&key_states.transpose(-2, -1)) * self.softmax_scale;
            let mask = Tensor::ones([t, t], (kind, self.device))
                .tril(0)
                .reshape([1, 1, t, t]);
            let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
            att.softmax(-1, kind).matmul(&value_states)
        };

        let y = y
            .transpose(1, 2)
            .contiguous()
            .reshape([b, t, self.num_heads * self.head_v_dim]);

        self.o_proj.forward(&y)
    }
}

#[derive(Debug)]
enum Attention {
    MQA(CausalSelfAttention),
    MLA(MLA)
}

impl Attention {
    fn new(vs: nn::Path, config: &ConsilienceConfig, use_sdpa: bool, comm: Option<Arc<Communicator>>) -> Self {
        if config.q_lora_rank.is_some() {
            Self::MLA(MLA::new(vs, config, use_sdpa, comm))
        } else {
            Self::MQA(CausalSelfAttention::new(
                &vs / "self_attn",
                config.num_attention_heads as i64,
                config.num_key_value_heads() as i64,
                config.hidden_size as i64,
                (config.max_position_embeddings + 1) as i64,
                use_sdpa,
                comm,
            ))
        }
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut RoPECache) -> Tensor {
        match self {
            Attention::MQA(mqa) => mqa.forward(x, index_pos, cache),
            Attention::MLA(mla) => mla.forward(x, index_pos, cache),
        }
    }
}

#[derive(Debug)]
struct Block {
    rms_1: RmsNorm,
    attn: Attention,
    rms_2: RmsNorm,
    mlp: Mlp,
}

impl Block {
    fn new(vs: nn::Path, config: &ConsilienceConfig, use_sdpa: bool, comm: Option<Arc<Communicator>>) -> Self {
        let rms_1 = RmsNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
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
            comm.clone(),
        );
        let attn = Attention::new(vs, config, use_sdpa, comm);
        Self {
            rms_1,
            attn,
            rms_2,
            mlp,
        }
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut RoPECache) -> Tensor {
        let x = self.attn.forward(&self.rms_1.forward(x), index_pos, cache) + x;
        self.mlp.forward(&self.rms_2.forward(&x)) + x
    }
}

#[derive(Debug)]
pub struct Consilience {
    wte: nn::Embedding,
    blocks: Vec<Block>,
    ln_f: RmsNorm,
}

impl Consilience {
    pub fn new(vs: nn::Path, config: &ConsilienceConfig, use_sdpa: bool, comm: Option<Arc<Communicator>>) -> Self {
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

    pub fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut RoPECache) -> Tensor {
        let mut x = self.wte.forward(x);
        for block in &self.blocks {
            x = block.forward(&x, index_pos, cache);
        }
        self.ln_f.forward(&x)
    }
}
