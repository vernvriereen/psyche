use std::io::Write;

use anyhow::{Error, Result};
use clap::Parser;
use psyche_data_provider::download_model_repo_sync;
use psyche_modeling::{
    auto_tokenizer, CausalLM, LlamaEosToks, LlamaForCausalLM, LogitsProcessor, Sampling,
    TokenOutputStream,
};
use tch::{Kind, Tensor};

const EOS_TOKEN: &str = "</s>";
const DEFAULT_PROMPT: &str = r"
EDWARD:
I wonder how our princely father 'scaped,
Or whether he be 'scaped away or no
From Clifford's and Northumberland's pursuit:
Had he been ta'en, we should have heard the news;
Had he been slain, we should have heard the news;
Or had he 'scaped, methinks we should have heard
The happy tidings of his good escape.
How fares my brother? why is he so sad?

RICHARD:
I cannot joy, until I be resolved
Where our right valiant father is become.
I saw him in the battle range about;
And watch'd him how he singled Clifford forth.
Methought he bore him in the thickest troop
As doth a lion in a herd of neat;
Or as a bear, encompass'd round with dogs,
Who having pinch'd a few and made them cry,
The rest stand all aloof, and bark at him.
So fared our father with his enemies;
So fled his enemies my warlike father:
Methinks, 'tis prize enough to be his son.
See how the morning opes her golden gates,
And takes her farewell of the glorious sun!
How well resembles it the prime of youth,
Trimm'd like a younker prancing to his love!

EDWARD:
Dazzle mine eyes, or do I see three suns?

RICHARD:
Three glorious suns, each one a perfect sun;
Not separated with the racking clouds,
But sever'd in a pale clear-shining sky.
See, see! they join, embrace, and seem to kiss,
As if they vow'd some league inviolable:
Now are they but one lamp, one light, one sun.
In this the heaven figures some event.

EDWARD:
'Tis wondrous strange, the like yet never heard of.
I think it cites us, brother, to the field,
That we, the sons of brave Plantagenet,
Each one already blazing by our meeds,
Should notwithstanding join our lights together
And over-shine the earth as this the world.
Whate'er it bodes, henceforward will I bear
Upon my target three fair-shining suns.
";

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "NousResearch/Llama-2-7b-hf")]
    model: Option<String>,

    #[arg(long, default_value_t = 0.6)]
    temperature: f64,

    #[arg(long)]
    top_p: Option<f64>,

    #[arg(long)]
    top_k: Option<usize>,

    #[arg(long)]
    max_tokens: Option<usize>,

    #[arg(long)]
    seed: Option<u64>,

    prompt: Option<String>,
}

fn main() -> Result<()> {
    let _no_grad = tch::no_grad_guard();
    let args = Args::parse();
    let repo_files = download_model_repo_sync(&args.model.unwrap(), None, None, None, true)?;
    let mut model =
        LlamaForCausalLM::from_pretrained(&repo_files, Some(Kind::BFloat16), None, None)?;
    let tokenizer = auto_tokenizer(&repo_files)?;
    let eos_token_id = model
        .config
        .eos_token_id
        .clone()
        .or_else(|| tokenizer.token_to_id(EOS_TOKEN).map(LlamaEosToks::Single));
    let prompt = args.prompt.as_ref().map_or(DEFAULT_PROMPT, |p| p.as_str());
    let mut tokens = tokenizer
        .encode(prompt, true)
        .map_err(Error::msg)?
        .get_ids()
        .into_iter()
        .map(|x| *x as i64)
        .collect::<Vec<_>>();
    let mut tokenizer = TokenOutputStream::new(tokenizer);
    print!("{prompt}");
    let mut logits_processor = {
        let temperature = args.temperature;
        let sampling = if temperature <= 0. {
            Sampling::ArgMax
        } else {
            match (args.top_k, args.top_p) {
                (None, None) => Sampling::All { temperature },
                (Some(k), None) => Sampling::TopK { k, temperature },
                (None, Some(p)) => Sampling::TopP { p, temperature },
                (Some(k), Some(p)) => Sampling::TopKThenTopP { k, p, temperature },
            }
        };
        LogitsProcessor::from_sampling(args.seed.unwrap_or(rand::random()), sampling)
    };
    let mut token_generated = 0;
    loop {
        if let Some(max_tokens) = args.max_tokens {
            if max_tokens >= token_generated {
                break;
            }
        }
        let input = Tensor::from_slice(&tokens).to(model.device).unsqueeze(0);
        let (logits, _) = model.forward(&input, None, Some(1));
        let logits = logits.squeeze();
        let next_token = logits_processor.sample(&logits)?;
        token_generated += 1;
        tokens.push(next_token as i64);

        match eos_token_id {
            Some(LlamaEosToks::Single(eos_tok_id)) if next_token == eos_tok_id => {
                break;
            }
            Some(LlamaEosToks::Multiple(ref eos_ids)) if eos_ids.contains(&next_token) => {
                break;
            }
            _ => (),
        }
        if let Some(t) = tokenizer.next_token(next_token)? {
            print!("{t}");
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
