use anyhow::Result;
use clap::Parser;
use psyche_training::LlamaForCausalLM;
use tch::Kind;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let model =
        LlamaForCausalLM::from_pretrained(&args.model.unwrap(), Some(Kind::BFloat16), None, None)?;
    Ok(())
}
