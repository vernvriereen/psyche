use anyhow::{bail, Result};
use psyche_data_provider::{Dataset, Split};

mod harness;
mod tasks;
mod traits;

pub use harness::{EvalTaskOptions, PreparedTask, PreparedTaskResult, Task, TaskType};
pub use tasks::{ArcChallenge, ArcEasy, Hellaswag, MMLUPro};

pub const ASCII_UPPERCASE: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

pub const ALL_TASK_NAMES: [&str; 4] = [
    ArcChallenge::name(),
    ArcEasy::name(),
    Hellaswag::name(),
    MMLUPro::name(),
];

pub fn load_dataset(repo_id: &str, split: Split, subset: Option<String>) -> Result<Dataset> {
    let repo_files = psyche_data_provider::download_dataset_repo_sync(repo_id, None, None, true)?;
    Dataset::load_dataset(&repo_files, Some(split), subset)
}

pub fn tasktype_from_name(name: &str) -> Result<TaskType> {
    match name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .as_str()
    {
        "arc_challenge" => ArcChallenge::load(),
        "arc_easy" => ArcEasy::load(),
        "hellaswag" => Hellaswag::load(),
        "mmlu_pro" => MMLUPro::load(),
        _ => bail!("Unknown task {name}"),
    }
}
