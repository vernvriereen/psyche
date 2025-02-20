use std::f32::consts::PI;

use tch::{Device, Kind, Tensor};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub enum RopeType {
    #[serde(rename = "llama3")]
    Llama3,
    #[default]
    #[serde(rename = "default")]
    Default,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RoPEConfig {
    pub factor: f32,
    pub low_freq_factor: f32,
    pub high_freq_factor: f32,
    pub original_max_position_embeddings: usize,
    pub rope_type: RopeType,
}

pub fn default_rope() -> f32 {
    10_000.0
}

fn calculate_default_inv_freq(
    hidden_size: usize,
    num_attention_heads: usize,
    rope_theta: f32,
) -> Vec<f32> {
    let head_dim = hidden_size / num_attention_heads;
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / rope_theta.powf(i as f32 / head_dim as f32))
        .collect()
}

#[derive(Debug)]
pub struct RoPECache {
    cos: Tensor,
    sin: Tensor,
}

impl RoPECache {
    pub fn new(
        kind: Kind,
        rope_config: &Option<RoPEConfig>,
        hidden_size: usize,
        num_attention_heads: usize,
        rope_theta: f32,
        max_position_embeddings: usize,
        device: &Device,
    ) -> Self {
        let theta = match rope_config {
            None
            | Some(RoPEConfig {
                rope_type: RopeType::Default,
                ..
            }) => calculate_default_inv_freq(hidden_size, num_attention_heads, rope_theta),
            Some(rope_scaling) => {
                let low_freq_wavelen = rope_scaling.original_max_position_embeddings as f32
                    / rope_scaling.low_freq_factor;
                let high_freq_wavelen = rope_scaling.original_max_position_embeddings as f32
                    / rope_scaling.high_freq_factor;

                calculate_default_inv_freq(hidden_size, num_attention_heads, rope_theta)
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

        let idx_theta =
            Tensor::arange((max_position_embeddings + 1) as i64, (Kind::Float, *device))
                .reshape([(max_position_embeddings + 1) as i64, 1])
                .matmul(&theta.reshape([1i64, theta.numel() as i64]));
        // This is different from the paper, see:
        // https://github.com/huggingface/transformers/blob/6112b1c6442aaf7affd2b0676a1cd4eee30c45cf/src/transformers/models/llama/modeling_llama.py#L112
        let cos = idx_theta.cos().to_kind(kind);
        let sin = idx_theta.sin().to_kind(kind);
        Self { cos, sin }
    }
}

pub fn rotate_half(xs: &Tensor) -> Tensor {
    let last_dim = *xs.size().last().unwrap();
    let xs1 = xs.narrow(-1, 0, last_dim / 2);
    let xs2 = xs.narrow(-1, last_dim / 2, last_dim - last_dim / 2);
    Tensor::cat(&[&xs2.neg(), &xs1], -1)
}

impl RoPECache {
    pub fn apply_rotary_emb(&self, x: &Tensor, index_pos: i64) -> Tensor {
        let (_b_sz, _, seq_len, _hidden_size) = x.size4().unwrap();
        let cos = self.cos.narrow(0, index_pos, seq_len);
        let sin = self.sin.narrow(0, index_pos, seq_len);
        let cos = Tensor::cat(&[&cos, &cos], -1);
        let sin = Tensor::cat(&[&sin, &sin], -1);
        let cos = cos.unsqueeze(0).unsqueeze(0);
        let sin = sin.unsqueeze(0).unsqueeze(0);
        (x * cos) + (rotate_half(x) * sin)
    }
}
