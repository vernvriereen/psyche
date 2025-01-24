mod auto_tokenizer;
mod batcher;
mod distro;
mod dummy;
mod fp32_gradient_accumulator;
mod llama;
mod llama_for_causal_lm;
mod safetensor_utils;
mod sampling;
mod tensor_parallelism;
mod token_output_stream;
mod traits;

pub use auto_tokenizer::{auto_tokenizer, AutoTokenizerError};
pub use batcher::Batcher;
pub use distro::{CompressDCT, Distro, DistroResult, TransformDCT};
pub use dummy::DummyModel;
pub use fp32_gradient_accumulator::Fp32GradientAccumulator;
pub use llama::{get_parameter_names, Cache, Config, Llama, LlamaEosToks};
pub use llama_for_causal_lm::{
    LlamaConfig, LlamaForCausalLM, LoadLlamaForCausalLMError, PretrainedSource,
};
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

#[cfg(test)]
pub fn set_torch_rng_seed() {
    use rand::Rng;

    let seed: i64 = rand::thread_rng().gen();
    tch::manual_seed(seed);
    println!("torch seed set to: {}", seed);
}
