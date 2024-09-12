use anyhow::{Error, Result};
use std::path::PathBuf;
use tokenizers::Tokenizer;

pub fn auto_tokenizer(repo_files: &[PathBuf]) -> Result<Tokenizer> {
    match repo_files.iter().find(|x| x.ends_with("tokenizer.json")) {
        Some(path) => Ok(Tokenizer::from_file(path.as_path()).map_err(Error::msg)?),
        None => Err(Error::msg("Could not find tokenizer.json")),
    }
}
