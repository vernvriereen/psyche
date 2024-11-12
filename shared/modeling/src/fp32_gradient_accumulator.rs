use tch::{Device, Kind, Tensor};

pub struct Fp32GradientAccumulator {
    parameters: Vec<(Tensor, (i64, i64))>,
    fp32_grads: Tensor,
}

impl Fp32GradientAccumulator {
    pub fn new(parameters: &[Tensor], device: Device) -> Self {
        let mut total_numel: i64 = 0;

        let parameters = parameters
            .iter()
            .filter_map(|parameter| match parameter.requires_grad() {
                true => {
                    let numel = parameter.numel() as i64;
                    let ret = (
                        parameter.shallow_clone(),
                        (total_numel, total_numel + numel),
                    );
                    total_numel += numel;
                    Some(ret)
                }
                false => None,
            })
            .collect::<Vec<_>>();

        let fp32_grads = Tensor::zeros(&[total_numel], (Kind::Float, device));

        Self {
            parameters,
            fp32_grads,
        }
    }

    pub fn accumulate_gradients(&mut self, accumulation_step: bool) {
        for (param, (start, end)) in &self.parameters {
            let mut grad = param.grad();

            let mut grad_slice = self.fp32_grads.slice(0, *start, *end, 1);
            let _ = grad_slice.g_add_(&grad.to_kind(Kind::Float).view([-1]));

            if accumulation_step {
                grad.copy_(&grad_slice.to_kind(param.kind()).view_as(&param));
            } else {
                let _ = grad.zero_();
            }
        }
    }

    pub fn zero_grad(&mut self) {
        let _ = self.fp32_grads.zero_();
        for (param, _) in &self.parameters {
            let _ = param.grad().zero_();
        }
    }

    pub fn get_full_grad_buffer(&self) -> &Tensor {
        &self.fp32_grads
    }

    pub fn scale_gradients(&mut self, scale: f64) {
        let _ = self.fp32_grads.g_mul_scalar_(scale);
    }

    pub fn clip_grad_norm(&mut self, max_norm: f64) {
        let total_norm: f64 = self.fp32_grads.norm().try_into().unwrap();
        if total_norm > max_norm {
            let scale = max_norm / (total_norm + 1e-6);
            self.scale_gradients(scale);
        }
    }
}
