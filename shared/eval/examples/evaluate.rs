use anyhow::Result;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{MMLUPro, Task};
use psyche_modeling::{auto_tokenizer, LlamaForCausalLM};
use tch::{Device, Kind};

fn main() -> Result<()> {
    let mut task = Task::new(MMLUPro::load()?, 5, 42);
    let repo = download_model_repo_sync("NousResearch/Llama-2-7b-hf", None, None, None, true)?;
    let mut model =
        LlamaForCausalLM::from_pretrained(&repo, Some(Kind::BFloat16), None, Some(Device::Cuda(0)))?;
    let tokenizer = auto_tokenizer(&repo)?;
    let score = task.run(&mut model, &tokenizer, false);
    println!("{task}: {score:.3}");
    Ok(())
}
