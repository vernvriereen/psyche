mod attention;
mod auto_config;
mod auto_tokenizer;
mod batcher;
mod causal_language_model;
mod distro;
mod dummy;
mod fp32_gradient_accumulator;
mod models;
mod rope;
mod safetensor_utils;
mod sampling;
mod tensor_parallelism;
mod token_output_stream;
mod traits;

pub use attention::CausalSelfAttention;
pub use auto_config::{
    AttentionImplementation, AutoConfig, ModelConfig, ModelLoadError, PretrainedSource,
};
pub use auto_tokenizer::{auto_tokenizer, AutoTokenizerError};
pub use batcher::Batcher;
pub use causal_language_model::{CausalLanguageModel, LanguageModelConfig, LanguageModelForward};
pub use distro::{CompressDCT, Distro, DistroResult, TransformDCT};
pub use dummy::DummyModel;
pub use fp32_gradient_accumulator::Fp32GradientAccumulator;
pub use models::*;
pub use rope::{default_rope, RoPECache, RoPEConfig};
pub use safetensor_utils::{
    load_safetensors_into_variables, save_tensors_into_safetensors, LoadSafetensorsError,
    SaveSafetensorsError,
};
pub use sampling::{LogitsProcessor, Sampling};
pub use tensor_parallelism::{
    unsharded_cpu_variables, AllReduce, ColumnParallelLinear, Communicator, CommunicatorId,
    CudaSynchronize, RowParallelLinear,
};
pub use token_output_stream::TokenOutputStream;
pub use traits::{CausalLM, ConcreteCausalLM};

#[allow(unused)]
pub fn set_torch_rng_seed() {
    use rand::Rng;

    let seed: i64 = rand::thread_rng().gen();
    tch::manual_seed(seed);
    println!("torch seed set to: {}", seed);
}
