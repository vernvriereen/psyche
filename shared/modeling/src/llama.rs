use std::f32::consts::PI;
use std::rc::Rc;
use tch::nn::{self, Module, Shard};
use tch::{Device, Kind, Tensor};

use crate::{Communicator, TensorParallelRowLinear};

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub enum Llama3RopeType {
    #[serde(rename = "llama3")]
    Llama3,
    #[default]
    #[serde(rename = "default")]
    Default,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct Llama3RopeConfig {
    pub factor: f32,
    pub low_freq_factor: f32,
    pub high_freq_factor: f32,
    pub original_max_position_embeddings: usize,
    pub rope_type: Llama3RopeType,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum LlamaEosToks {
    Single(u32),
    Multiple(Vec<u32>),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub rms_norm_eps: f64,
    pub rope_theta: f32,
    pub bos_token_id: Option<u32>,
    pub eos_token_id: Option<LlamaEosToks>,
    pub rope_scaling: Option<Llama3RopeConfig>,
    pub max_position_embeddings: usize,
    pub use_sdpa: bool,
}

pub struct Cache {
    cos: Tensor,
    sin: Tensor,
}

fn calculate_default_inv_freq(cfg: &Config) -> Vec<f32> {
    let head_dim = cfg.hidden_size / cfg.num_attention_heads;
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / cfg.rope_theta.powf(i as f32 / head_dim as f32))
        .collect()
}

impl Cache {
    pub fn new(kind: Kind, config: &Config, device: &Device) -> Self {
        let theta = match &config.rope_scaling {
            None
            | Some(Llama3RopeConfig {
                rope_type: Llama3RopeType::Default,
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

fn repeat_kv(xs: Tensor, n_rep: i64) -> Tensor {
    if n_rep == 1 {
        xs
    } else {
        let (b_sz, n_kv_head, seq_len, head_dim) = xs.size4().unwrap();
        Tensor::cat(&vec![&xs; n_rep as usize], 2).reshape([
            b_sz,
            n_kv_head * n_rep,
            seq_len,
            head_dim,
        ])
    }
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
        let norm_xs = (xs.pow_tensor_scalar(2).mean_dim(-1, true, Kind::Float) + self.eps).sqrt();
        let xs_normed = xs / norm_xs;
        (&self.weight * xs_normed).to_kind(kind)
    }
}

#[derive(Debug)]
struct Mlp {
    pub(crate) c_fc1: nn::Linear,
    pub(crate) c_fc2: nn::Linear,
    pub(crate) c_proj: TensorParallelRowLinear,
}

impl Mlp {
    fn new(vs: nn::Path, n_embd: i64, n_hidden: i64, comm: Option<Rc<Communicator>>) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            shard: comm.as_ref().map(|comm| Shard {
                dim: 0,
                rank: comm.rank(),
                world_size: comm.world_size(),
            }),
            ..Default::default()
        };
        let c_fc1 = nn::linear(&vs / "gate_proj", n_embd, n_hidden, c);
        let c_fc2 = nn::linear(&vs / "up_proj", n_embd, n_hidden, c);
        let c_proj = TensorParallelRowLinear::new(
            nn::linear(
                &vs / "down_proj",
                n_hidden,
                n_embd,
                nn::LinearConfig {
                    shard: comm.as_ref().map(|comm| Shard {
                        dim: 1,
                        rank: comm.rank(),
                        world_size: comm.world_size(),
                    }),
                    ..c
                },
            ),
            comm,
        );
        Self {
            c_fc1,
            c_fc2,
            c_proj,
        }
    }
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let xs = xs.apply(&self.c_fc1).silu() * xs.apply(&self.c_fc2);
        xs.apply(&self.c_proj)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct CausalSelfAttention {
    q_proj: nn::Linear,
    k_proj: nn::Linear,
    v_proj: nn::Linear,
    o_proj: TensorParallelRowLinear,
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
        comm: Option<Rc<Communicator>>,
    ) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            shard: comm.as_ref().map(|comm| Shard {
                dim: 0,
                rank: comm.rank(),
                world_size: comm.world_size(),
            }),
            ..Default::default()
        };
        let tp_size = comm.as_ref().map(|x| x.world_size() as i64).unwrap_or(1);
        let head_dim = n_embd / n_head;
        let size_q = head_dim * n_head;
        let size_kv = head_dim * n_kvheads;
        let q_proj = nn::linear(&vs / "q_proj", n_embd, size_q, c);
        let k_proj = nn::linear(&vs / "k_proj", n_embd, size_kv, c);
        let v_proj = nn::linear(&vs / "v_proj", n_embd, size_kv, c);
        let o_proj = TensorParallelRowLinear::new(
            nn::linear(
                &vs / "o_proj",
                size_q,
                n_embd,
                nn::LinearConfig {
                    shard: comm.as_ref().map(|comm| Shard {
                        dim: 1,
                        rank: comm.rank(),
                        world_size: comm.world_size(),
                    }),
                    ..c
                },
            ),
            comm,
        );
        Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            head_dim,
            n_kvhead: n_kvheads,
            n_embd,
            n_max_seq_len,
            device: vs.device(),
            use_sdpa,
            tp_size,
        }
    }

    fn apply_rotary_emb(&self, x: &Tensor, index_pos: i64, cache: &Cache) -> Tensor {
        let (_b_sz, _, seq_len, _hidden_size) = x.size4().unwrap();
        let cos = cache.cos.narrow(0, index_pos, seq_len);
        let sin = cache.sin.narrow(0, index_pos, seq_len);
        let cos = Tensor::cat(&[cos.copy(), cos], -1);
        let sin = Tensor::cat(&[sin.copy(), sin], -1);
        let cos = cos.narrow(0, 0, seq_len);
        let sin = sin.narrow(0, 0, seq_len);
        let cos = cos.unsqueeze(0).unsqueeze(0);
        let sin = sin.unsqueeze(0).unsqueeze(0);
        (x * cos) + (rotate_half(x) * sin)
    }

    fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut Cache) -> Tensor {
        let (b, t, c) = x.size3().unwrap();
        let kind = x.kind();
        let q = self.q_proj.forward(x);
        let k = self.k_proj.forward(x);
        let v = self.v_proj.forward(x);
        let local_n_head = self.n_head / self.tp_size;
        let local_n_kvhead = self.n_kvhead / self.tp_size;
        let q = q
            .reshape([b, t, local_n_head, self.head_dim])
            .transpose(1, 2);
        let k = k
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);
        let v = v
            .reshape([b, t, local_n_kvhead, self.head_dim])
            .transpose(1, 2);
        let q = self.apply_rotary_emb(&q, index_pos, cache).to_kind(kind);
        let k = self.apply_rotary_emb(&k, index_pos, cache).to_kind(kind);
        let k = repeat_kv(k, local_n_head / local_n_kvhead);
        let v = repeat_kv(v, local_n_head / local_n_kvhead);
        let y = if self.use_sdpa {
            let att =
                Tensor::scaled_dot_product_attention::<Tensor>(&q, &k, &v, None, 0.0, t > 1, None);
            att.transpose(1, 2).reshape([b, t, c / self.tp_size])
        } else {
            let k_shape = k.size();
            let att: Tensor =
                q.matmul(&k.transpose(-2, -1)) / (*k_shape.last().unwrap() as f64).sqrt();
            let mask = Tensor::ones([t, t], (kind, self.device))
                .tril(0)
                .reshape([1, 1, t, t]);
            let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
            let y = att.softmax(-1, kind).matmul(&v);
            y.transpose(1, 2).reshape([b, t, c / self.tp_size])
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
    fn new(vs: nn::Path, config: &Config, comm: Option<Rc<Communicator>>) -> Self {
        let rms_1 = RmsNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let attn = CausalSelfAttention::new(
            &vs / "self_attn",
            config.num_attention_heads as i64,
            config.num_key_value_heads as i64,
            config.hidden_size as i64,
            (config.max_position_embeddings + 1) as i64,
            config.use_sdpa,
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
    pub fn new(vs: nn::Path, config: &Config, comm: Option<Rc<Communicator>>) -> Self {
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
            .map(|i| Block::new(&vs / "model" / "layers" / i, config, comm.clone()))
            .collect::<Vec<_>>();
        Self {
            wte,
            blocks,
            ln_f,
        }
    }

    pub fn forward(&self, x: &Tensor, index_pos: i64, cache: &mut Cache) -> Tensor {
        let mut x = self.wte.forward(x);
        for block in &self.blocks {
            x = block.forward(&x, index_pos, cache);
        }
        self.ln_f.forward(&x)
    }
}

