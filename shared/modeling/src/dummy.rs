use std::time::Duration;

use tch::{nn::VarStore, Device, Kind};

use crate::{CausalLM, ConcreteCausalLM};

#[derive(Debug)]
pub struct DummyModel {
    var_store: VarStore,
    training_delay_secs: Duration,
}

impl Default for DummyModel {
    fn default() -> Self {
        Self::new(2)
    }
}

impl DummyModel {
    pub fn new(training_delay: u64) -> Self {
        Self {
            var_store: VarStore::new(Device::cuda_if_available()),
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
