use crate::app::{AppBuilder, AppParams};

use anchor_client::solana_sdk::{
    signature::{EncodableKey, Keypair},
    signer::Signer,
};
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use psyche_client::{
    exercise_sdpa_if_needed, print_identity_keys, read_identity_secret_key, TrainArgs,
};
use psyche_network::SecretKey;
use psyche_tui::LogOutput;
use std::path::PathBuf;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::{info, Level};

mod app;
mod backend;
mod network_identity;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    ShowIdentity {
        identity_secret_key_path: Option<PathBuf>,
    },
    CreateIdentity {
        save_path: PathBuf,
    },
    Train {
        #[clap(short, long, env)]
        wallet_private_key_path: Option<PathBuf>,

        #[clap(flatten)]
        args: TrainArgs,
    },
}

async fn async_main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::ShowIdentity {
            identity_secret_key_path,
        } => print_identity_keys(identity_secret_key_path.as_ref()),
        Commands::CreateIdentity { save_path } => {
            let identity_secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(&save_path, identity_secret_key.secret().as_bytes())?;
            print_identity_keys(Some(&save_path))?;
            println!("Wrote secret key to {}", save_path.display());
            Ok(())
        }
        Commands::Train {
            wallet_private_key_path,
            args,
        } => {
            exercise_sdpa_if_needed();

            let hub_read_token = std::env::var("HF_TOKEN").ok();
            let checkpoint_upload_info = args.checkpoint_config()?;
            let eval_tasks = args.eval_tasks()?;

            psyche_tui::init_logging(
                // if args.tui {
                //     LogOutput::TUI
                // } else {
                //     LogOutput::Console
                // },
                LogOutput::Console,
                Level::INFO,
                args.write_log.clone(),
            );

            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let identity_secret_key: SecretKey =
                read_identity_secret_key(args.identity_secret_key_path.as_ref())?
                    .unwrap_or_else(|| SecretKey::generate(&mut rand::rngs::OsRng));

            let wallet_keypair = Arc::new(match std::env::var("RAW_WALLET_PRIVATE_KEY").ok() {
                Some(raw_wallet_private_key) => Keypair::from_base58_string(&raw_wallet_private_key),
                None => match wallet_private_key_path {
                    Some(wallet_private_key_path) => match Keypair::read_from_file(wallet_private_key_path) {
                        Ok(wallet_keypair) => wallet_keypair,
                        Err(err) => bail!("{}", err),
                    },
                    None => bail!("No wallet private key! Must pass --wallet-private-key-path or set RAW_WALLET_PRIVATE_KEY")
                }
            });

            let wandb_info = args.wandb_info(format!(
                "{}-{}",
                args.run_id.clone(),
                wallet_keypair.pubkey()
            ))?;

            // let (cancel, tx_tui_state) = maybe_start_render_loop(
            //     args.tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)),
            // )?;

            let (mut app, p2p, state_options) = AppBuilder::new(AppParams {
                //cancel,
                //tx_tui_state,
                identity_secret_key,
                wallet_keypair,
                run_id: args.run_id,
                p2p_port: args.bind_p2p_port,
                data_parallelism: args.data_parallelism,
                tensor_parallelism: args.tensor_parallelism,
                micro_batch_size: args.micro_batch_size,
                write_gradients_dir: args.write_gradients_dir,
                eval_task_max_docs: args.eval_task_max_docs,
                eval_tasks,
                checkpoint_upload_info,
                hub_read_token,
                wandb_info,
                optim_stats: args.optim_stats_steps,
                grad_accum_in_fp32: args.grad_accum_in_fp32,
                dummy_training_delay_secs: args.dummy_training_delay_secs,
            })
            .build()
            .await
            .unwrap();

            app.run(p2p, state_options).await
        }
    }
}

fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .thread_stack_size(10 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(async_main())
}
