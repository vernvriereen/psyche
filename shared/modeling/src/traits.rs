use tch::{Device, Tensor};

pub trait CausalLM {
    fn forward(&mut self, x: &Tensor, labels: Option<&Tensor>, num_logits_to_keep: Option<i64>) -> (Tensor, Option<Tensor>);
    fn bos_token_id(&self) -> Option<i64>;
    fn device(&self) -> Device;
}
