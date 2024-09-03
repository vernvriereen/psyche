use tch::nn::{self, Module};
use tch::{Device, Kind, Tensor};

pub struct Config {
    pub vocab_size: usize,
    pub n_layer: usize,
    pub n_head: usize,
    pub n_embd: usize,
    pub seq_len: usize,
}

#[derive(Debug)]
struct RmsNorm {
    scale: Tensor,
    size: i64,
}

impl RmsNorm {
    fn new(vs: nn::Path, size: i64) -> Self {
        let scale = vs.zeros("scale", &[size]);
        Self { scale, size }
    }
}

impl Module for RmsNorm {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let kind = xs.kind();
        let norm_xs = (xs * xs).mean_dim(-1, true, Kind::Float);
        let xs_normed = xs * (norm_xs + 1e-5).rsqrt();
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
    fn new(vs: nn::Path, n_embd: i64) -> Self {
        let n_hidden = 8 * n_embd / 3;
        let n_hidden = (n_hidden - 1) / 256 * 256 + 256;
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };
        let c_fc1 = nn::linear(&vs / "c_fc1", n_embd, n_hidden, c);
        let c_fc2 = nn::linear(&vs / "c_fc2", n_embd, n_hidden, c);
        let c_proj = nn::linear(&vs / "c_proj", n_hidden, n_embd, c);
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

#[derive(Debug)]
struct CausalSelfAttention {
    c_attn: nn::Linear,
    c_proj: nn::Linear,
    n_head: i64,
    n_embd: i64,
    device: Device,
}

impl CausalSelfAttention {
    fn new(vs: nn::Path, n_head: i64, n_embd: i64) -> Self {
        let c = nn::LinearConfig {
            bias: false,
            ..Default::default()
        };
        let c_attn = nn::linear(&vs / "c_attn", n_embd, 3 * n_embd, c);
        let c_proj = nn::linear(&vs / "c_proj", n_embd, n_embd, c);
        Self {
            c_attn,
            c_proj,
            n_head,
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
        let qkv = self.c_attn.forward(x);
        let n_embd = self.n_embd;
        let q = qkv.slice(2, 0, n_embd, 1);
        let k = qkv.slice(2, n_embd, 2 * n_embd, 1);
        let v = qkv.slice(2, 2 * n_embd, 3 * n_embd, 1);
        let target_dim = [b, t, self.n_head, c / self.n_head];
        let k = k.reshape(target_dim).transpose(1, 2);
        let q = q.reshape(target_dim).transpose(1, 2);
        let v = v.reshape(target_dim).transpose(1, 2);
        let q = self.apply_rotary_emb(&q, freqs_cis);
        let k = self.apply_rotary_emb(&k, freqs_cis);
        let k_shape = k.size();
        let att: Tensor = q.matmul(&k.transpose(-2, -1)) / (*k_shape.last().unwrap() as f64).sqrt();
        let mask = Tensor::ones([t, t], (kind, self.device))
            .tril(0)
            .reshape([1, 1, t, t]);
        let att = att.masked_fill(&mask.eq(0.), f64::NEG_INFINITY);
        let y = att.softmax(-1, kind).matmul(&v);
        let y = y.transpose(1, 2).reshape([b, t, c]);
        self.c_proj.forward(&y)
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
        let rms_1 = RmsNorm::new(&vs / "rms_1", config.n_embd as i64);
        let attn =
            CausalSelfAttention::new(&vs / "attn", config.n_head as i64, config.n_embd as i64);
        let rms_2 = RmsNorm::new(&vs / "rms_2", config.n_embd as i64);
        let mlp = Mlp::new(&vs / "mlp", config.n_embd as i64);
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
        let lm_head = nn::linear(
            &vs / "lm_head",
            config.n_embd as i64,
            config.vocab_size as i64,
            c,
        );
        let wte = nn::embedding(
            &vs / "transformer" / "wte",
            config.vocab_size as i64,
            config.n_embd as i64,
            Default::default(),
        );
        let blocks = (0..config.n_layer)
            .map(|i| Block::new(&vs / "transformer" / "h" / i, config))
            .collect::<Vec<_>>();
        let ln_f = RmsNorm::new(&vs / "transformer" / "ln_f", config.n_embd as i64);
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
    let n_elem = config.n_embd / config.n_head;
    let theta: Vec<_> = (0..n_elem)
        .step_by(2)
        .map(|i| 1f32 / 10000f32.powf(i as f32 / n_elem as f32))
        .collect();
    let arange: Vec<_> = (0..config.seq_len).map(|c| c as f32).collect();
    let theta = Tensor::from_slice(&theta);
    let arange = Tensor::from_slice(&arange);
    let idx_theta = arange.outer(&theta);
    let shape = [1, 1, config.seq_len as i64, n_elem as i64 / 2, 1];
    let idx_theta_cos = idx_theta.cos().reshape(shape);
    let idx_theta_sin = idx_theta.sin().reshape(shape);
    Tensor::cat(&[&idx_theta_cos, &idx_theta_sin], -1)
}
