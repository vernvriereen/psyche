use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use tch::{
    nn::{VarStore, Variables},
    Device, Kind, Tensor,
};

use crate::{CausalLM, ConcreteCausalLM, EosToks};

#[derive(Debug)]
pub struct DummyModel {
    var_store: VarStore,
    training_delay_secs: Duration,
}

pub fn get_dummy_parameters() -> [&'static str; 12] {
    [
        "model.norm.weight",
        "model.layers.0.mlp.up_proj.weight",
        "model.layers.0.post_attention_layernorm.weight",
        "model.layers.0.self_attn.q_proj.weight",
        "model.embed_tokens.weight",
        "model.layers.0.self_attn.o_proj.weight",
        "model.layers.0.self_attn.v_proj.weight",
        "model.layers.0.self_attn.k_proj.weight",
        "model.layers.0.mlp.gate_proj.weight",
        "model.layers.0.mlp.down_proj.weight",
        "lm_head.weight",
        "model.layers.0.input_layernorm.weight",
    ]
}

impl Default for DummyModel {
    fn default() -> Self {
        Self::new(2)
    }
}

impl DummyModel {
    pub fn new(training_delay: u64) -> Self {
        let parameters = get_dummy_parameters();
        let named_variables: HashMap<String, Tensor> = parameters
            .into_iter()
            .map(|p| (p.to_string(), Tensor::zeros([1], tch::kind::FLOAT_CPU)))
            .collect();
        let variables = Variables {
            named_variables,
            shards: HashMap::new(),
            trainable_variables: Vec::new(),
        };
        let mut var_store = VarStore::new(Device::cuda_if_available());
        var_store.variables_ = Arc::new(Mutex::new(variables));
        Self {
            var_store,
            training_delay_secs: Duration::from_secs(training_delay),
        }
    }
}

impl CausalLM for DummyModel {
    fn forward(
        &mut self,
        x: &tch::Tensor,
        _labels: Option<&tch::Tensor>,
        _num_logits_to_keep: Option<i64>,
    ) -> (tch::Tensor, Option<tch::Tensor>) {
        let result = tch::Tensor::zeros([1], (Kind::BFloat16, x.device()));
        let loss = tch::Tensor::zeros([1], (Kind::BFloat16, x.device()));
        let loss = loss.set_requires_grad(true);
        let loss = loss.g_add_scalar(1.0);

        // sleep some time just to simulate training
        std::thread::sleep(self.training_delay_secs);
        (result, Some(loss))
    }

    fn bos_token_id(&self) -> Option<i64> {
        None
    }

    fn eos_token_ids(&self) -> Option<EosToks> {
        None
    }

    fn device(&self) -> tch::Device {
        Device::cuda_if_available()
    }
}

impl ConcreteCausalLM for DummyModel {
    fn variables(&self) -> &tch::nn::VarStore {
        &self.var_store
    }

    fn communicator(&self) -> Option<std::sync::Arc<crate::Communicator>> {
        None
    }
}
