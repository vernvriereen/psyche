use anyhow::Result;
use clap::Parser;
use psyche_data_provider::download_model_repo_sync;
use psyche_eval::{tasktype_from_name, EvalTaskOptions, Task, ALL_TASK_NAMES};
use psyche_modeling::{auto_tokenizer, CausalLM, LlamaForCausalLM};
use tch::{Device, Kind};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: String,

    #[arg(long, default_value_t = ALL_TASK_NAMES.join(","))]
    tasks: String,

    #[arg(long, default_value_t = 0)]
    num_fewshot: usize,

    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let tasks: Result<Vec<Task>> = args
        .tasks
        .split(",")
        .into_iter()
        .map(|x| tasktype_from_name(x).map(|y| Task::new(y, args.num_fewshot, args.seed)))
        .collect();
    let tasks = tasks?;
    let repo = download_model_repo_sync(&args.model, None, None, None, true)?;
    let mut model = LlamaForCausalLM::from_pretrained(
        &repo,
        Some(Kind::BFloat16),
        None,
        Some(Device::Cuda(0)),
        None,
        None,
    )?;
    let tokenizer = auto_tokenizer(&repo)?;
    for task in tasks {
        let name = format!("{task}");
        let result = task.prepare(&tokenizer, model.bos_token_id(), None).run(
            EvalTaskOptions {
                model: &mut model,
                skip_and_step_by: None,
                live_results: None,
                cancel: None,
                limit: None,
                loop_if_empty: false,
            },
            false,
        );

        println!("{}: {:?}", name, result.scores);
    }
    Ok(())
}
