use anyhow::Result;
use psyche_data_provider::{Dataset, Split};

mod harness;
mod tasks;
mod traits;

pub use tasks::{Hellaswag, MMLUPro};
pub use harness::{Task, TaskType};

pub fn load_dataset(repo_id: &str, split: Split) -> Result<Dataset> {
    let repo_files =
        psyche_data_provider::download_dataset_repo_sync(repo_id, None, None, true)?;
    Dataset::load_dataset(&repo_files, Some(split))
}

pub const ASCII_UPPERCASE: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];
