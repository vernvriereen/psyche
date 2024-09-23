use psyche_coordinator::model;
use psyche_core::NodeIdentity;
use psyche_data_provider::DataProviderTcpClient;
use psyche_modeling::LlamaForCausalLM;

pub struct Trainer<T: NodeIdentity> {
    _data: DataProviderTcpClient<T>,
    _model: LlamaForCausalLM,
}

impl<T: NodeIdentity> Trainer<T> {
    pub fn new(
        data: DataProviderTcpClient<T>,
        model: LlamaForCausalLM,
    ) -> Self {
        Self {
            _data: data,
            _model: model,
        }
    }

    pub fn train(self, _llm: model::LLM) -> Trainer<T> {
        self
    }
}
