use crate::traits::{Document, LogLikelihoodTask};
use indicatif::{ProgressBar, ProgressStyle};
use psyche_modeling::CausalLM;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{cmp::Ordering, fmt::Display};
use tch::{Kind, Tensor};
use tokenizers::Tokenizer;

pub enum TaskType {
    LogLikelihood(Box<dyn LogLikelihoodTask>),
}

pub struct Task {
    task_type: TaskType,
    num_fewshot: usize,
    rand: ChaCha8Rng,
}

impl Task {
    pub fn new(task_type: TaskType, num_fewshot: usize, random_seed: u64) -> Self {
        let mut seed = [0u8; 32];
        seed[24..32].copy_from_slice(&random_seed.to_be_bytes());
        Task {
            task_type,
            num_fewshot,
            rand: ChaCha8Rng::from_seed(seed),
        }
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.task_type {
            TaskType::LogLikelihood(x) => write!(f, "{x}"),
        }
    }
}

struct TokenizedDocument {
    text: Vec<i64>,
    choices: Vec<Vec<i64>>,
    answer: usize,
}

impl TokenizedDocument {
    pub fn from_document(doc: Document, tokenizer: &Tokenizer) -> Self {
        let text = tokenizer
            .encode(doc.text, false)
            .unwrap()
            .get_ids()
            .iter()
            .map(|x| *x as i64)
            .collect::<Vec<_>>();
        let choices = doc
            .choices
            .into_iter()
            .map(|x| {
                tokenizer
                    .encode(x, false)
                    .unwrap()
                    .get_ids()
                    .iter()
                    .map(|x| *x as i64)
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            text,
            choices,
            answer: doc.answer,
        }
    }
}

impl Task {
    pub fn run<M: CausalLM>(&mut self, model: &mut M, tokenizer: &Tokenizer, quiet: bool) -> f32 {
        let _no_grad = tch::no_grad_guard();
        match &self.task_type {
            TaskType::LogLikelihood(llh) => Task::run_log_likelihood(
                model,
                tokenizer,
                &llh,
                self.num_fewshot,
                &mut self.rand,
                quiet,
            ),
        }
    }

    fn run_log_likelihood<M: CausalLM>(
        model: &mut M,
        tokenizer: &Tokenizer,
        llh: &Box<dyn LogLikelihoodTask>,
        num_fewshot: usize,
        rand: &mut ChaCha8Rng,
        quiet: bool,
    ) -> f32 {
        if !quiet {
            println!("Preparing {}", llh);
        }
        let docs = llh.get_documents();
        // test all answers are one character along until length stuff is done
        assert_eq!(
            docs.iter()
                .fold(0, |acc, e| acc + e.choices[e.answer].len()),
            docs.len()
        );
        let fewshot = if num_fewshot > 0 {
            let mut fewshot_docs = llh.get_fewshot_documents();
            fewshot_docs.shuffle(rand);
            fewshot_docs
                .into_iter()
                .take(num_fewshot)
                .map(|x| format!("{}{}", x.text, x.choices[x.answer]))
                .collect::<Vec<_>>()
                .join("\n\n")
                + "\n\n"
        } else {
            String::new()
        };
        let mut tokenized_fewshot = match model.bos_token_id() {
            Some(bos_token_id) => vec![bos_token_id],
            None => Vec::new(),
        };
        tokenized_fewshot.append(
            &mut tokenizer
                .encode(fewshot, false)
                .unwrap()
                .get_ids()
                .iter()
                .map(|x| *x as i64)
                .collect::<Vec<_>>(),
        );
        let docs = docs
            .into_iter()
            .map(|x| TokenizedDocument::from_document(x, tokenizer))
            .collect::<Vec<_>>();
        let pbar = match quiet {
            true => None,
            false => {
                println!("Running {llh}");
                let pbar = ProgressBar::new(docs.len() as u64);
                pbar.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"));
                Some(pbar)
            }
        };
        let mut acc_num = 0f32;
        let mut acc_denom = 0f32;
        for mut doc in docs {
            let mut ids = tokenized_fewshot.clone();
            ids.append(&mut doc.text);
            let ids = Tensor::from_slice(&ids).to(model.device()).unsqueeze(0);
            let (logits, _) = model.forward(&ids, None, Some(1));
            let logits = logits.squeeze();
            let answer_probs: Vec<f32> = doc
                .choices
                .into_iter()
                .map(|x| logits.get(x[0]).to_kind(Kind::Float).try_into().unwrap())
                .collect::<Vec<_>>();
            let selected = answer_probs
                .into_iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))
                .map(|(index, _)| index)
                .unwrap();
            if selected == doc.answer {
                acc_num += 1.;
            }
            acc_denom += 1.;
            if let Some(pbar) = pbar.as_ref() {
                let acc = acc_num / acc_denom;
                pbar.set_message(format!("{acc:.3}"));
                pbar.inc(1)
            }
        }
        acc_num / acc_denom
    }
}
