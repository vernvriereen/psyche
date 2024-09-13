use anyhow::Result;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{Hellaswag, MMLUPro, Task};
use psyche_modeling::{auto_tokenizer, LlamaForCausalLM};
use tch::{Device, Kind};

fn main() -> Result<()> {
    let tasks = vec![
        Task::new(Hellaswag::load()?, 0, 42),
        Task::new(MMLUPro::load()?, 0, 42),
    ];
    let repo = download_model_repo_sync("unsloth/Meta-Llama-3.1-8B", None, None, None, true)?;
    let mut model = LlamaForCausalLM::from_pretrained(
        &repo,
        Some(Kind::BFloat16),
        None,
        Some(Device::Cuda(0)),
    )?;
    let tokenizer = auto_tokenizer(&repo)?;
    for task in tasks {
        let name = format!("{task}");
        let scores = task
            .prepare(&mut model, &tokenizer, false, None)
            .run(&mut model, false);
        println!("{name}: {scores:?}");
    }
    Ok(())
}
