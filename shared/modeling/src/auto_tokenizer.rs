use std::path::PathBuf;
use thiserror::Error;
use tokenizers::Tokenizer;

#[derive(Error, Debug)]
pub enum AutoTokenizerError {
    #[error("Failed to load tokenizer from tokenizer.json")]
    CouldntLoadTokenizer(#[from] tokenizers::Error),

    #[error("Could not find tokenizer.json")]
    FileNotFound,
}

pub fn auto_tokenizer(repo_files: &[PathBuf]) -> Result<Tokenizer, AutoTokenizerError> {
    match repo_files.iter().find(|x| x.ends_with("tokenizer.json")) {
        Some(path) => Ok(Tokenizer::from_file(path.as_path())?),
        None => Err(AutoTokenizerError::FileNotFound),
    }
}
