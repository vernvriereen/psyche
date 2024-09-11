use tch::nn::{self, Module};
use tch::{Device, Kind, Tensor};

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

#[allow(dead_code)]
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
}

#[derive(Debug)]
struct RmsNorm {
    scale: Tensor,
    size: i64,
    eps: f64,
}

impl RmsNorm {
    fn new(vs: nn::Path, size: i64, eps: f64) -> Self {
        let scale = vs.zeros("scale", &[size]);
        Self { scale, size, eps }
    }
}

impl Module for RmsNorm {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let kind = xs.kind();
        let norm_xs = (xs * xs).mean_dim(-1, true, Kind::Float);
        let xs_normed = xs * (norm_xs + self.eps).rsqrt();
        let scale = self.scale.reshape([1, 1, self.size]);
        scale * xs_normed.to_kind(kind)
    }
}

#[derive(Debug)]
struct Mlp {
    c_fc1: nn::Linear,
    c_fc2: nn::Linear,
    c_proj: nn::Linear,
}

impl Mlp {
    fn new(vs: nn::Path, n_embd: i64, n_hidden: i64) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };
        let c_fc1 = nn::linear(&vs / "gate_proj", n_embd, n_hidden, c);
        let c_fc2 = nn::linear(&vs / "up_proj", n_embd, n_hidden, c);
        let c_proj = nn::linear(&vs / "down_proj", n_hidden, n_embd, c);
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
    o_proj: nn::Linear,
    n_head: i64,
    n_kvhead: i64,
    n_embd: i64,
    head_dim: i64,
    device: Device,
}

impl CausalSelfAttention {
    fn new(vs: nn::Path, n_head: i64, n_kvheads: i64, n_embd: i64) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };
        let head_dim = n_embd / n_head;
        let size_q = head_dim * n_head;
        let size_kv = head_dim * n_kvheads;
        let q_proj = nn::linear(&vs / "q_proj", n_embd, size_q, c);
        let k_proj = nn::linear(&vs / "k_proj", n_embd, size_kv, c);
        let v_proj = nn::linear(&vs / "n_proj", n_embd, size_kv, c);
        let o_proj = nn::linear(&vs / "o_proj", size_q, n_embd, c);
        Self {
            q_proj,
            k_proj,
            v_proj,
            o_proj,
            n_head,
            head_dim,
            n_kvhead: n_kvheads,
            n_embd,
            device: vs.device(),
        }
    }

    fn apply_rotary_emb(&self, x: &Tensor, freqs_cis: &Tensor) -> Tensor {
        let mut dims = x.size();
        let v = dims.pop().unwrap();
        dims.push(v / 2);
        dims.push(2);
        let x = x.reshape(&dims);
        let re_x = x.slice(-1, 0, 1, 1);
        let im_x = x.slice(-1, 1, 2, 1);
        let re_f = freqs_cis.slice(-1, 0, 1, 1);
        let im_f = freqs_cis.slice(-1, 1, 2, 1);
        let re = &re_x * &re_f - &im_x * &im_f;
        let im = &re_x * &im_f + &im_x * &re_f;
        let rope = Tensor::cat(&[&re, &im], -1);
        // TODO: Add the flatten op.
        let mut dims = rope.size();
        let v1 = dims.pop().unwrap();
        let v2 = dims.pop().unwrap();
        dims.push(v1 * v2);
        rope.reshape(&dims)
    }

    fn forward(&self, x: &Tensor, freqs_cis: &Tensor) -> Tensor {
        let (b, t, c) = x.size3().unwrap();
        let kind = x.kind();
        let q = self.q_proj.forward(x);
        let k = self.k_proj.forward(x);
        let v = self.v_proj.forward(x);
        let k = k
            .reshape([b, t, self.n_head, self.head_dim])
            .transpose(1, 2);
        let q = q
            .reshape([b, t, self.n_kvhead, self.head_dim])
            .transpose(1, 2);
        let v = v
            .reshape([b, t, self.n_kvhead, self.head_dim])
            .transpose(1, 2);
        let q = self.apply_rotary_emb(&q, freqs_cis).to_kind(kind);
        let k = self.apply_rotary_emb(&k, freqs_cis).to_kind(kind);
        let k = repeat_kv(k, self.n_head / self.n_kvhead);
        let v = repeat_kv(v, self.n_head / self.n_kvhead);
        let mask = Tensor::ones([t, t], (kind, self.device))
            .tril(0)
            .reshape([1, 1, t, t]);
        let att = Tensor::scaled_dot_product_attention(&q, &k, &v, Some(mask), 0.0, true, None);
        let y = att.transpose(1, 2).reshape([b, t, c]);
        // let k_shape = k.size();
        // let att: Tensor = q.matmul(&k.transpose(-2, -1)) / (*k_shape.last().unwrap() as f64).sqrt();
        // let mask = Tensor::ones([t, t], (kind, self.device))
        //     .tril(0)
        //     .reshape([1, 1, t, t]);
        // let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
        // let y = att.softmax(-1, kind).matmul(&v);
        // let y = y.transpose(1, 2).reshape([b, t, c]);
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
    fn new(vs: nn::Path, config: &Config) -> Self {
        let rms_1 = RmsNorm::new(
            &vs / "input_layernorm",
            config.hidden_size as i64,
            config.rms_norm_eps as f64,
        );
        let attn = CausalSelfAttention::new(
            &vs / "self_attn",
            config.num_attention_heads as i64,
            config.num_key_value_heads as i64,
            config.hidden_size as i64,
        );
        let rms_2 = RmsNorm::new(
            &vs / "rms_2",
            config.hidden_size as i64,
            config.rms_norm_eps as f64,
        );
        let mlp = Mlp::new(
            &vs / "mlp",
            config.hidden_size as i64,
            config.intermediate_size as i64,
        );
        Self {
            rms_1,
            attn,
            rms_2,
            mlp,
        }
    }

    fn forward(&self, x: &Tensor, freqs_cis: &Tensor) -> Tensor {
        let x = self.attn.forward(&self.rms_1.forward(x), freqs_cis) + x;
        self.mlp.forward(&self.rms_2.forward(&x)) + x
    }
}

#[derive(Debug)]
pub struct Llama {
    wte: nn::Embedding,
    blocks: Vec<Block>,
    ln_f: RmsNorm,
    lm_head: nn::Linear,
    freqs_cis: Tensor,
}

impl Llama {
    pub fn new(vs: nn::Path, config: &Config) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };
        let wte = nn::embedding(
            &vs / "model" / "embed_tokens",
            config.vocab_size as i64,
            config.hidden_size as i64,
            Default::default(),
        );
        let lm_head = nn::linear(
            &vs / "lm_head",
            config.hidden_size as i64,
            config.vocab_size as i64,
            c,
        );
        let ln_f = RmsNorm::new(
            &vs / "model" / "norm",
            config.hidden_size as i64,
            config.rms_norm_eps,
        );
        let blocks = (0..config.num_hidden_layers)
            .map(|i| Block::new(&vs / "model" / "layers" / i, config))
            .collect::<Vec<_>>();
        let freqs_cis = precompute_freqs_cis(config).to(vs.device());
        Self {
            wte,
            blocks,
            ln_f,
            lm_head,
            freqs_cis,
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let mut x = self.wte.forward(x);
        for block in self.blocks.iter() {
            x = block.forward(&x, &self.freqs_cis);
        }
        let x = self.ln_f.forward(&x);
        self.lm_head.forward(&x)
    }
}

fn precompute_freqs_cis(config: &Config) -> Tensor {
    let n_elem = config.hidden_size / config.num_attention_heads;
    let theta: Vec<_> = (0..n_elem)
        .step_by(2)
        .map(|i| 1f32 / config.rope_theta.powf(i as f32 / n_elem as f32))
        .collect();
    let arange: Vec<_> = (0..config.max_position_embeddings)
        .map(|c| c as f32)
        .collect();
    let theta = Tensor::from_slice(&theta);
    let arange = Tensor::from_slice(&arange);
    let idx_theta = arange.outer(&theta);
    let shape = [
        1,
        1,
        config.max_position_embeddings as i64,
        n_elem as i64 / 2,
        1,
    ];
    let idx_theta_cos = idx_theta.cos().reshape(shape);
    let idx_theta_sin = idx_theta.sin().reshape(shape);
    Tensor::cat(&[&idx_theta_cos, &idx_theta_sin], -1)
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
