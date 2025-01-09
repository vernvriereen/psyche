use anyhow::Result;
use clap::{Parser, Subcommand};
use psyche_core::BatchId;
use psyche_data_provider::{
    http::{FileURLs, HttpDataProvider},
    Shuffle, TokenSize, TokenizedDataProvider,
};
use std::path::PathBuf;
use tokenizers::Tokenizer;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// File size in bytes
    #[arg(long)]
    file_size: usize,

    /// Sequence length
    #[arg(long, default_value = "2048")]
    sequence_length: usize,

    /// Token size in bytes
    #[arg(long, default_value = "2")]
    token_size: usize,

    /// Batch IDs to retrieve (comma-separated)
    #[arg(long, use_value_delimiter = true)]
    batch_ids: Vec<u64>,

    /// Optional tokenizer path for decoding output
    #[arg(long)]
    tokenizer: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Use HTTP data provider with URL template
    Template {
        /// URL template with {} placeholder (e.g., "http://example.com/{}.ds")
        template: String,
        /// Start index
        #[arg(long, default_value = "0")]
        start: usize,
        /// End index
        #[arg(long)]
        end: usize,

        // number of zeros to left-pad to
        #[arg(long, default_value = "0")]
        left_pad_zeros: usize,
    },
    /// Use HTTP data provider with URL list
    Urls {
        /// List of data URLs, in order (e.g., "http://example.com/1.ds", "http://example.com/2.ds")
        urls: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let token_size: TokenSize = cli.token_size.try_into()?;

    let batch_ids: Vec<BatchId> = cli.batch_ids.into_iter().map(BatchId::from_u64).collect();
    if batch_ids.is_empty() {
        anyhow::bail!("At least one batch ID must be specified");
    }

    let urls = match cli.command {
        Commands::Template {
            template,
            start,
            left_pad_zeros,
            end,
        } => FileURLs::from_template(template, start, left_pad_zeros, end - start)?,
        Commands::Urls { urls } => {
            if urls.is_empty() {
                anyhow::bail!("at least one URL must be passed");
            }
            FileURLs::from_list(&urls)
        }
    };
    let mut provider = HttpDataProvider::new(
        urls,
        cli.file_size,
        token_size,
        cli.sequence_length,
        Shuffle::DontShuffle,
    )
    .await?;
    let samples = provider.get_samples(&batch_ids).await?;

    // Output handling
    if let Some(tokenizer_path) = cli.tokenizer {
        let tokenizer = Tokenizer::from_file(tokenizer_path).expect("tokenizer exists");
        for (i, sample) in samples.iter().enumerate() {
            println!("=== Batch {} ===", batch_ids[i]);
            let decoded = tokenizer
                .decode(&sample.iter().map(|&x| x as u32).collect::<Vec<_>>(), true)
                .expect("tokenizer decode worked");
            println!("{}", decoded);
            println!();
        }
    } else {
        for (i, sample) in samples.iter().enumerate() {
            println!("=== Batch {} ===", batch_ids[i]);
            println!("{:?}", sample);
            println!();
        }
    }

    Ok(())
}
