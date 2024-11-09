use crate::app::{AppBuilder, AppParams, Tabs, TAB_NAMES};

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Parser, Subcommand};
use psyche_client::{BatchShuffleType, CheckpointSaveInfo, HubUploadInfo, WandBInfo};
use psyche_eval::tasktype_from_name;
use psyche_network::SecretKey;
use psyche_tui::{maybe_start_render_loop, LogOutput};
use std::path::PathBuf;
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::{info, Level};

mod app;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    ShowIdentity {
        secret_key: PathBuf,
    },
    Train {
        #[clap(long)]
        secret_key: Option<PathBuf>,

        #[clap(short, long, env)]
        bind_p2p_port: Option<u16>,

        #[clap(
            long,
            action = ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            require_equals = false,
            env
        )]
        tui: bool,

        #[clap(long, env)]
        run_id: String,

        #[clap(long, env)]
        server_addr: String,

        #[clap(long, default_value_t = 1, env)]
        data_parallelism: usize,

        #[clap(long, default_value_t = 1, env)]
        tensor_parallelism: usize,

        #[clap(long, env)]
        micro_batch_size: Option<usize>,

        /// If provided, every shared gradient this client sees will be written to this directory.
        #[clap(long, env)]
        write_gradients_dir: Option<PathBuf>,

        #[clap(long, env)]
        eval_tasks: Option<String>,

        #[clap(long, default_value_t = 0, env)]
        eval_fewshot: usize,

        #[clap(long, default_value_t = 42, env)]
        eval_seed: u64,

        #[clap(long, env)]
        eval_task_max_docs: Option<usize>,

        #[clap(long, env)]
        checkpoint_dir: Option<PathBuf>,

        #[clap(long, env)]
        hub_repo: Option<String>,

        #[clap(long, env)]
        wandb_project: Option<String>,

        #[clap(long, env)]
        wandb_run: Option<String>,

        #[clap(long, env)]
        wandb_entity: Option<String>,

        /// a 32-byte long hex string. WARNING: providing the same shuffle to two nodes will result in a LOT of duplicated & discarded training work.
        #[clap(long, env)]
        fixed_batch_shuffle: Option<String>,

        #[clap(long, env)]
        write_log: Option<PathBuf>,
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
            fixed_batch_shuffle,
            write_log,
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

            let batch_shuffle_type = match fixed_batch_shuffle {
                None => BatchShuffleType::Random,
                Some(seed) => BatchShuffleType::Fixed(
                    hex::decode(seed)?
                        .try_into()
                        .map_err(|_| anyhow!("batch shuffle seed is not valid 32 bytes!"))?,
                ),
            };

            let hub_read_token = std::env::var("HF_TOKEN").ok();

            let checkpoint_upload_info = match (&hub_read_token, hub_repo, checkpoint_dir) {
                (Some(token), Some(repo), Some(dir)) => Some(CheckpointSaveInfo {
                    checkpoint_dir: dir,
                    hub_upload: Some(HubUploadInfo {
                        hub_repo: repo,
                        hub_token: token.to_string(),
                    }),
                }),
                (None, Some(_), Some(_)) => {
                    bail!("hub-repo and checkpoint-dir set, but no HF_TOKEN env variable.")
                }
                (_, Some(_), None) => {
                    bail!("--hub-repo was set, but no --checkpoint-dir was passed!")
                }
                (_, None, Some(dir)) => Some(CheckpointSaveInfo {
                    checkpoint_dir: dir,
                    hub_upload: None,
                }),
                (_, None, _) => None,
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
                write_log,
            );

            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let private_key: SecretKey = secret_key
                .map(|k| {
                    SecretKey::try_from_openssh(
                        String::from_utf8(std::fs::read(k).unwrap()).unwrap(),
                    )
                    .unwrap()
                })
                .unwrap_or_else(SecretKey::generate);

            let (cancel, tx_tui_state) =
                maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

            AppBuilder::new(AppParams {
                cancel,
                private_key,
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
                checkpoint_upload_info,
                hub_read_token,
                wandb_info,
                batch_shuffle_type,
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
