use crate::Communicator;
use std::sync::Arc;
use tch::{nn::VarStore, Device, Tensor};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum EosToks {
    Single(i64),
    Multiple(Vec<i64>),
}

/// This trait is for any Causal Language Model that can be inferred,
/// and thus can have backprop run on it.
/// Its internal implementation is completely hidden, so this can be impl'd
/// for a wrapper struct that does something like data parallelism.
pub trait CausalLM: Send + std::fmt::Debug {
    fn forward(
        &mut self,
        x: &Tensor,
        labels: Option<&Tensor>,
        num_logits_to_keep: Option<i64>,
    ) -> (Tensor, Option<Tensor>);
    fn bos_token_id(&self) -> Option<i64>;
    fn eos_token_ids(&self) -> Option<EosToks>;
    fn device(&self) -> Device;
}

/// This trait is only for Causal Language models that are concretely instatiated -
/// i.e. they have a VarStore that holds parameters, and they may have a Communicator for inter-gpu communication.
pub trait ConcreteCausalLM: CausalLM {
    fn variables(&self) -> &VarStore;
    fn communicator(&self) -> Option<Arc<Communicator>>;
}
