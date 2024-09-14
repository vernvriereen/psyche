use crate::traits::{Document, LogLikelihoodTask};
use indicatif::{ProgressBar, ProgressStyle};
use psyche_modeling::CausalLM;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{collections::HashMap, fmt::Display};
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

enum PreparedTaskType {
    LogLikelihood {
        docs: Vec<TokenizedLLHDocument>,
        tokenized_fewshot: Vec<i64>,
    },
}

pub struct PreparedTask {
    prepared_task_type: PreparedTaskType,
    name: String,
    num: usize,
}

struct TokenizedLLHDocument {
    text: Vec<i64>,
    choices: Vec<Vec<i64>>,
    answer: usize,
}

impl TokenizedLLHDocument {
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
    pub fn prepare(
        mut self,
        tokenizer: &Tokenizer,
        bos_token_id: Option<i64>,
        quiet: bool,
        limit: Option<usize>,
    ) -> PreparedTask {
        let name = format!("{}", &self);
        if !quiet {
            println!("Preparing {name}");
        }
        match self.task_type {
            TaskType::LogLikelihood(llh) => {
                let mut docs = llh.get_documents();
                docs.shuffle(&mut self.rand);
                if let Some(limit) = limit {
                    docs.truncate(limit);
                }
                let fewshot = if self.num_fewshot > 0 {
                    let mut fewshot_docs = llh.get_fewshot_documents();
                    fewshot_docs.shuffle(&mut self.rand);
                    fewshot_docs
                        .into_iter()
                        .take(self.num_fewshot)
                        .map(|x| format!("{}{}", x.text, x.choices[x.answer]))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                        + "\n\n"
                } else {
                    String::new()
                };
                let mut tokenized_fewshot = match bos_token_id {
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
                    .map(|x| TokenizedLLHDocument::from_document(x, tokenizer))
                    .collect::<Vec<_>>();
                PreparedTask {
                    name,
                    num: docs.len(),
                    prepared_task_type: PreparedTaskType::LogLikelihood {
                        docs,
                        tokenized_fewshot,
                    },
                }
            }
        }
    }
}

impl PreparedTask {
    pub fn run<M: CausalLM>(&self, model: &mut M, quiet: bool) -> HashMap<String, f32> {
        let pbar = match quiet {
            true => None,
            false => {
                println!("Running {}", self.name);
                let pbar = ProgressBar::new(self.num as u64);
                pbar.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"));
                Some(pbar)
            }
        };

        match &self.prepared_task_type {
            PreparedTaskType::LogLikelihood {
                docs,
                tokenized_fewshot,
            } => Self::run_log_likelihood(model, docs, tokenized_fewshot, pbar),
        }
    }

    fn run_log_likelihood<M: CausalLM>(
        model: &mut M,
        docs: &Vec<TokenizedLLHDocument>,
        tokenized_fewshot: &Vec<i64>,
        pbar: Option<ProgressBar>,
    ) -> HashMap<String, f32> {
        let mut acc_num = 0f32;
        let mut acc_norm_num = 0f32;
        let mut acc_denom = 0f32;
        for doc in docs {
            let mut context = tokenized_fewshot.clone();
            context.extend_from_slice(&doc.text);
            let mut scores: Vec<(f32, bool)> = Vec::new();
            if doc.choices.iter().all(|x| x.len() == 1) {
                let ids = Tensor::from_slice(&context).to(model.device()).unsqueeze(0);
                let (logits, _) = model.forward(&ids, None, Some(1));
                let logits = logits.squeeze().log_softmax(-1, None);
                let greedy: i64 = logits.argmax(-1, false).try_into().unwrap();
                let index =
                    Tensor::from_slice(&doc.choices.iter().map(|x| x[0]).collect::<Vec<_>>())
                        .to(logits.device());
                let logits = logits.gather(-1, &index, false);
                let logits: Vec<f32> = logits.try_into().unwrap();
                scores.extend(
                    logits
                        .into_iter()
                        .zip(doc.choices.iter())
                        .map(|(score, choice)| (score, choice[0] == greedy)),
                );
            } else {
                for choice in &doc.choices {
                    let mut ids = context.clone();
                    ids.extend_from_slice(&choice);
                    let ids = Tensor::from_slice(&ids).to(model.device()).unsqueeze(0);
                    let (logits, _) = model.forward(&ids, None, Some((choice.len() + 1) as i64));
                    let logits =
                        logits
                            .log_softmax(-1, None)
                            .squeeze()
                            .slice(0, 0, choice.len() as i64, 1);
                    let greedy_tokens: Vec<i64> = logits.argmax(-1, false).try_into().unwrap();
                    let exact_match = greedy_tokens.eq(choice);
                    let index = Tensor::from_slice(&choice)
                        .to(logits.device())
                        .unsqueeze(-1);
                    let logits = logits.gather(-1, &index, false);
                    let loglikelihood: f32 = logits.sum(Kind::Float).try_into().unwrap();
                    scores.push((loglikelihood, exact_match));
                }
            }
            let selected: i64 = Tensor::from_slice(&scores.iter().map(|x| x.0).collect::<Vec<_>>())
                .argmax(-1, false)
                .try_into()
                .unwrap();
            let selected_norm: i64 = Tensor::from_slice(
                &scores
                    .iter()
                    .enumerate()
                    .map(|(idx, x)| x.0 / doc.choices[idx].len() as f32)
                    .collect::<Vec<_>>(),
            )
            .argmax(-1, false)
            .try_into()
            .unwrap();

            if selected as usize == doc.answer {
                acc_num += 1.;
            }
            if selected_norm as usize == doc.answer {
                acc_norm_num += 1.;
            }
            acc_denom += 1.;

            if let Some(pbar) = &pbar {
                pbar.set_message(format!("acc_norm: {:.3}", acc_norm_num / acc_denom));
                pbar.inc(1);
            }
        }
        HashMap::from([
            ("acc".to_owned(), acc_num / acc_denom),
            ("acc_norm".to_owned(), acc_norm_num / acc_denom),
        ])
    }
}
