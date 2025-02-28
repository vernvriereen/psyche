use psyche_coordinator::{model, Coordinator};
use psyche_core::{BoundedQueue, NodeIdentity};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokenizers::Tokenizer;
use tracing::warn;
use wandb::{DataValue, LogData};

use super::evals::EvalRunner;

pub struct StatsLogger {
    tokenizer: Arc<Tokenizer>,
    wandb_run: Option<Arc<wandb::Run>>,
    eval_runner: EvalRunner,

    round_durations: BoundedQueue<Duration, 16>,
    losses: Vec<f32>,
    last_optim_stats: HashMap<String, f64>,
    eval_history: HashMap<String, Vec<f64>>,
    lr_scheduler: model::AnyLearningRateScheduler,
}

impl StatsLogger {
    pub fn new(
        tokenizer: Arc<Tokenizer>,
        eval_runner: EvalRunner,
        lr_scheduler: model::AnyLearningRateScheduler,
        wandb_run: Option<wandb::Run>,
    ) -> Self {
        Self {
            tokenizer,
            wandb_run: wandb_run.map(Arc::new),
            losses: Vec::new(),
            round_durations: Default::default(),
            eval_runner,
            lr_scheduler,
            eval_history: HashMap::new(),
            last_optim_stats: HashMap::new(),
        }
    }

    pub fn publish_round_stats<T: NodeIdentity>(
        &self,
        state: &Coordinator<T>,
        node_info: &HashMap<String, DataValue>,
    ) {
        let mut round_log = LogData::new();

        round_log.insert("_step", state.progress.step);

        if let Some(loss) = self.losses().last() {
            round_log.insert("train/loss", *loss);
            round_log.insert("train/perplexity", perplexity(*loss));
            round_log.insert("train/confidence", self.confidence(*loss));
        }
        round_log.insert("train/lr", self.lr_scheduler.get_lr(state.progress.step));

        round_log.insert("train/total_tokens", total_tokens(state));
        round_log.insert("train/tokens_per_sec", self.global_tokens_per_second(state));

        round_log.insert("coordinator/num_clients", state.epoch_state.clients.len());
        round_log.insert("coordinator/epoch", state.progress.epoch);
        round_log.insert(
            "coordinator/round",
            state.current_round().map(|x| x.height).unwrap_or_default(),
        );

        for (key, val) in self.current_eval_results() {
            round_log.insert(
                format!(
                    "eval/{}",
                    key.to_lowercase()
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ),
                val,
            );
        }

        for (name, value) in &self.last_optim_stats {
            round_log.insert(format!("optim/{name}"), *value);
        }

        round_log.insert("p2p/nodes", node_info.clone());

        if let Some(run) = self.wandb_run.clone() {
            tokio::spawn(async move {
                run.log(round_log).await;
            });
        }
    }

    pub fn push_round_stats(
        &mut self,
        round_losses: &[f32],
        round_duration: Duration,
        optim_stats: HashMap<String, f64>,
    ) -> f32 {
        let loss = round_losses.iter().sum::<f32>() / round_losses.len() as f32;
        self.losses.push(loss);

        self.round_durations.push(round_duration);

        self.last_optim_stats = optim_stats;
        loss
    }

    /// only call this once per step
    /// take the current eval results and push them
    pub fn push_eval_results(&mut self) {
        for (key, value) in self.current_eval_results() {
            self.eval_history
                .entry(key.clone())
                .or_default()
                .push(value);
        }
    }

    pub fn eval_history(&self) -> &HashMap<String, Vec<f64>> {
        &self.eval_history
    }

    pub fn losses(&self) -> &[f32] {
        &self.losses
    }

    pub fn global_tokens_per_second<T: NodeIdentity>(&self, state: &Coordinator<T>) -> f32 {
        match self.round_durations.is_empty() {
            true => 0.,
            false => match &state.model {
                model::Model::LLM(llm) => match llm.data_type {
                    model::LLMTrainingDataType::Pretraining => {
                        let tokens = state.config.batches_per_round as u32
                            * state.config.data_indicies_per_batch as u32
                            * llm.max_seq_len;
                        let seconds = self
                            .round_durations
                            .iter()
                            .fold(0f32, |acc, ele| acc + ele.as_secs_f32());
                        tokens as f32 / (seconds / self.round_durations.len() as f32)
                    }
                    model::LLMTrainingDataType::Finetuning => todo!(),
                },
            },
        }
    }

    pub fn current_eval_results(&self) -> HashMap<String, f64> {
        self.eval_runner
            .tasks()
            .iter()
            .flatten()
            .flat_map(|eval_task| {
                let task = eval_task.task();
                let metric_name: &str = task.main_metric_name();
                let task_name = task.name();
                match eval_task.results().sample(metric_name) {
                    Some(metric) => Some((task_name.to_owned(), metric)),
                    None => {
                        warn!("{} missing metric {}", task_name, metric_name);
                        None
                    }
                }
            })
            .collect()
    }

    // normalized metric for how "confident" a model is, regardless of vocab size.
    // 1.0 indicates completely certain (no loss), 0.0 indicates random guessing, negative values are worse than guessing
    fn confidence(&self, loss: f32) -> f32 {
        let max_entropy = (self.tokenizer.get_vocab_size(false) as f32).log2();
        1.0 - (loss / max_entropy)
    }
}

fn total_tokens<T: NodeIdentity>(state: &Coordinator<T>) -> u64 {
    state
        .current_round()
        .map(|y| y.data_index)
        .unwrap_or_default()
        * match &state.model {
            model::Model::LLM(llm) => match llm.data_type {
                model::LLMTrainingDataType::Pretraining => llm.max_seq_len as u64,
                model::LLMTrainingDataType::Finetuning => todo!(),
            },
        }
}

fn perplexity(loss: f32) -> f32 {
    loss.exp()
}
