use std::f32::consts::PI;

use tch::{Device, Kind, Tensor};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default, PartialEq)]
pub enum RoPEType {
    #[serde(rename = "llama3")]
    Llama3,
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "yarn")]
    YaRN,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RoPEConfig {
    pub factor: Option<f32>,
    pub low_freq_factor: Option<f32>,
    pub high_freq_factor: Option<f32>,
    pub original_max_position_embeddings: Option<usize>,
    #[serde(rename = "type")]
    pub rope_type: RoPEType,
    pub beta_fast: Option<f32>,
    pub beta_slow: Option<f32>,
    pub mscale: Option<f32>,
    pub mscale_all_dim: Option<f32>,
}

pub fn default_rope() -> f32 {
    10_000.0
}

fn calculate_default_inv_freq(head_dim: usize, rope_theta: f32) -> Vec<f32> {
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / rope_theta.powf(i as f32 / head_dim as f32))
        .collect()
}

fn yarn_find_correction_dim(
    num_rotations: f32,
    dim: usize,
    base: f32,
    max_position_embeddings: usize,
) -> f32 {
    (dim as f32
        * (max_position_embeddings as f32 / (num_rotations * 2.0 * std::f32::consts::PI)).ln())
        / (2.0 * base.ln())
}

fn yarn_find_correction_range(
    low_rot: f32,
    high_rot: f32,
    dim: usize,
    base: f32,
    max_position_embeddings: usize,
) -> (usize, usize) {
    let low =
        yarn_find_correction_dim(low_rot, dim, base, max_position_embeddings).floor() as usize;
    let high =
        yarn_find_correction_dim(high_rot, dim, base, max_position_embeddings).ceil() as usize;
    (low.max(0), high.min(dim - 1))
}

fn yarn_linear_ramp_mask(min: usize, max: usize, dim: usize, device: Device) -> Tensor {
    let max = if min == max { max + 1 } else { max }; // Prevent singularity
    let t = Tensor::arange(dim as i64, (Kind::Float, device));
    let linear_func = (&t - min as f64) / (max as f64 - min as f64);
    linear_func.clamp(0.0, 1.0)
}

pub fn yarn_get_mscale(scale: f32, mscale: f32) -> f32 {
    if scale <= 1.0 {
        1.0
    } else {
        0.1 * mscale * scale.ln() + 1.0
    }
}

#[derive(Debug)]
pub struct RoPECache {
    pub cos: Tensor,
    pub sin: Tensor,
}

impl RoPECache {
    pub fn new(
        kind: Kind,
        rope_config: &Option<RoPEConfig>,
        head_dim: usize,
        rope_theta: f32,
        max_position_embeddings: usize,
        device: &Device,
    ) -> Self {
        let inv_freq = calculate_default_inv_freq(head_dim, rope_theta);

        let (inv_freq, mscale) = match rope_config {
            None
            | Some(RoPEConfig {
                rope_type: RoPEType::Default,
                ..
            }) => (Tensor::from_slice(&inv_freq).to(*device), None),
            Some(RoPEConfig {
                rope_type: RoPEType::Llama3,
                original_max_position_embeddings,
                factor,
                low_freq_factor,
                high_freq_factor,
                ..
            }) => {
                let original_max_position_embeddings =
                    original_max_position_embeddings.unwrap() as f32;
                let factor = factor.unwrap();
                let low_freq_factor = low_freq_factor.unwrap();
                let high_freq_factor = high_freq_factor.unwrap();
                let low_freq_wavelen = original_max_position_embeddings / low_freq_factor;
                let high_freq_wavelen = original_max_position_embeddings / high_freq_factor;

                let inv_freq = inv_freq
                    .into_iter()
                    .map(|freq| {
                        let wavelen = 2. * PI / freq;
                        if wavelen < high_freq_wavelen {
                            freq
                        } else if wavelen > low_freq_wavelen {
                            freq / factor
                        } else {
                            let smooth = (original_max_position_embeddings / wavelen
                                - low_freq_factor)
                                / (high_freq_factor - low_freq_factor);
                            (1. - smooth) * freq / factor + smooth * freq
                        }
                    })
                    .collect::<Vec<_>>();

                (Tensor::from_slice(&inv_freq).to(*device), None)
            }
            Some(RoPEConfig {
                rope_type: RoPEType::YaRN,
                factor,
                beta_fast,
                beta_slow,
                original_max_position_embeddings,
                mscale,
                mscale_all_dim,
                ..
            }) => {
                let freq_extra = Tensor::from_slice(&inv_freq).to(*device);

                let theta_inter =
                    calculate_default_inv_freq(head_dim, rope_theta * factor.unwrap());
                let freq_inter = Tensor::from_slice(&theta_inter).to(*device);

                let (low, high) = yarn_find_correction_range(
                    beta_fast.unwrap(),
                    beta_slow.unwrap(),
                    head_dim,
                    rope_theta,
                    original_max_position_embeddings.unwrap(),
                );

                // Create interpolation mask
                let inv_freq_mask = 1.0 - yarn_linear_ramp_mask(low, high, head_dim / 2, *device);

                let inv_freq = &freq_inter * (1.0 - &inv_freq_mask) + &freq_extra * &inv_freq_mask;

                // Calculate scaling factor
                let mscale = yarn_get_mscale(factor.unwrap(), mscale.unwrap())
                    / yarn_get_mscale(factor.unwrap(), mscale_all_dim.unwrap_or(1.));

                (inv_freq, Some(mscale as f64))
            }
        };

        let idx_theta =
            Tensor::arange((max_position_embeddings + 1) as i64, (Kind::Float, *device))
                .reshape([(max_position_embeddings + 1) as i64, 1])
                .matmul(&inv_freq.reshape([1i64, inv_freq.numel() as i64]));
        // This is different from the paper, see:
        // https://github.com/huggingface/transformers/blob/6112b1c6442aaf7affd2b0676a1cd4eee30c45cf/src/transformers/models/llama/modeling_llama.py#L112
        let mut cos = idx_theta.cos();
        if let Some(mscale) = mscale {
            let _ = cos.g_mul_scalar_(mscale);
        };
        let mut sin = idx_theta.sin();
        if let Some(mscale) = mscale {
            let _ = sin.g_mul_scalar_(mscale);
        }
        Self {
            cos: cos.to_kind(kind),
            sin: sin.to_kind(kind),
        }
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
