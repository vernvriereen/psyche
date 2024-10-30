use crate::app::{AppBuilder, AppParams, Tabs, TAB_NAMES};

use anyhow::{bail, Result};
use clap::{ArgAction, Parser, Subcommand};
use psyche_client::WandBInfo;
use psyche_eval::tasktype_from_name;
use psyche_network::SecretKey;
use psyche_tui::{maybe_start_render_loop, LogOutput};
use std::path::PathBuf;
use tokio::runtime::Builder;
use tracing::{info, Level};

mod app;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    ShowIdentity {
        secret_key: PathBuf,
    },
    Train {
        #[clap(long)]
        secret_key: Option<PathBuf>,

        #[clap(short, long)]
        bind_p2p_port: Option<u16>,

        #[clap(
            long,
            action = ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            require_equals = false
        )]
        tui: bool,

        #[clap(long)]
        run_id: String,

        #[clap(long)]
        server_addr: String,

        #[clap(long, default_value_t = 1)]
        data_parallelism: usize,

        #[clap(long, default_value_t = 1)]
        tensor_parallelism: usize,

        #[clap(long)]
        micro_batch_size: Option<usize>,

        /// If provided, every shared gradient this client sees will be written to this directory.
        #[clap(long)]
        write_gradients_dir: Option<PathBuf>,

        #[clap(long)]
        eval_tasks: Option<String>,

        #[clap(long, default_value_t = 0)]
        eval_fewshot: usize,

        #[clap(long, default_value_t = 42)]
        eval_seed: u64,

        #[clap(long)]
        eval_task_max_docs: Option<usize>,

        #[clap(long)]
        checkpoint_dir: Option<PathBuf>,

        #[clap(long)]
        hub_repo: Option<String>,

        #[clap(long)]
        wandb_project: Option<String>,

        #[clap(long)]
        wandb_run: Option<String>,

        #[clap(long)]
        wandb_entity: Option<String>,
    },
}

async fn async_main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::ShowIdentity { secret_key } => {
            println!(
                "{}",
                SecretKey::try_from_openssh(String::from_utf8(std::fs::read(secret_key)?)?)?
                    .public()
            );
            Ok(())
        }
        Commands::Train {
            secret_key,
            bind_p2p_port,
            tui,
            run_id,
            server_addr,
            data_parallelism,
            tensor_parallelism,
            micro_batch_size,
            write_gradients_dir,
            eval_tasks,
            eval_fewshot,
            eval_seed,
            eval_task_max_docs,
            checkpoint_dir,
            hub_repo,
            wandb_run,
            wandb_entity,
            wandb_project,
        } => {
            #[cfg(target_os = "windows")]
            {
                // this is a gigantic hack to cover that called sdpa prints out
                // "Torch was not compiled with flash attention." via TORCH_WARN
                // on Windows, which screws with the TUI.
                // it's done once (really TORCH_WARN_ONCE), so elicit that behavior
                // before starting anything else
                use tch::Tensor;
                let device = tch::Device::Cuda(0);
                let _ = Tensor::scaled_dot_product_attention::<Tensor>(
                    &Tensor::from_slice2(&[[0.]]).to(device),
                    &Tensor::from_slice2(&[[0.]]).to(device),
                    &Tensor::from_slice2(&[[0.]]).to(device),
                    None,
                    0.0,
                    false,
                    None,
                );
            }

            let hub_token = match &hub_repo {
                Some(_) => {
                    if checkpoint_dir.is_none() {
                        bail!("--checkpoint-dir must be set if --hub-repo is set");
                    }
                    match std::env::var("HF_TOKEN") {
                        Ok(hub_token) =>Some(hub_token),
                        Err(_) => bail!("HF_TOKEN environment variable must be set for checkpoint uploading to Hugging Face Hub")
                    }
                }
                None => None,
            };

            let wandb_info = match std::env::var("WANDB_API_KEY") {
                Ok(wandb_api_key) => Some(WandBInfo {
                    project: wandb_project.unwrap_or("psyche".to_string()),
                    run: wandb_run.unwrap_or(run_id.clone()),
                    entity: wandb_entity,
                    api_key: wandb_api_key,
                }),
                Err(_) => {
                    match wandb_entity.is_some() || wandb_run.is_some() || wandb_project.is_some() {
                        true => bail!(
                            "WANDB_API_KEY environment variable must be set for wandb integration"
                        ),
                        false => None,
                    }
                }
            };

            let eval_tasks = match eval_tasks {
                Some(eval_tasks) => {
                    let result: Result<Vec<psyche_eval::Task>> = eval_tasks
                        .split(",")
                        .map(|eval_task| {
                            tasktype_from_name(eval_task).map(|task_type| {
                                psyche_eval::Task::new(task_type, eval_fewshot, eval_seed)
                            })
                        })
                        .collect();
                    result?
                }
                None => Vec::new(),
            };

            psyche_tui::init_logging(
                if tui {
                    LogOutput::TUI
                } else {
                    LogOutput::Console
                },
                Level::INFO,
            );

            info!("Joining gossip room");

            let secret_key: SecretKey = secret_key
                .map(|k| {
                    SecretKey::try_from_openssh(
                        String::from_utf8(std::fs::read(k).unwrap()).unwrap(),
                    )
                    .unwrap()
                })
                .unwrap_or_else(SecretKey::generate);

            let tui = tui;

            let (cancel, tx_tui_state) =
                maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

            AppBuilder::new(AppParams {
                cancel,
                secret_key,
                server_addr,
                tx_tui_state,
                run_id,
                p2p_port: bind_p2p_port,
                data_parallelism,
                tensor_parallelism,
                micro_batch_size,
                write_gradients_dir,
                eval_task_max_docs,
                eval_tasks,
                checkpoint_dir,
                hub_repo,
                hub_token,
                wandb_info,
            })
            .run()
            .await
        }
    }
}

fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .build()
        .unwrap();
    runtime.block_on(async_main())
}
