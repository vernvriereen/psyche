use crate::{
    app::{AppBuilder, AppParams, Tabs, TAB_NAMES},
    backend::SolanaBackend,
};

use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        native_token::lamports_to_sol,
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
    Cluster,
};
use anyhow::{bail, Context, Result};
use bytemuck::Zeroable;
use clap::{Args, Parser, Subcommand};
use psyche_client::{
    exercise_sdpa_if_needed, print_identity_keys, read_identity_secret_key, TrainArgs,
};
use psyche_coordinator::{model::Model, CoordinatorConfig};
use psyche_network::SecretKey;
use psyche_solana_coordinator::ClientId;
use psyche_tui::{maybe_start_render_loop, LogOutput};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::runtime::Builder;
use tracing::{info, Level};

mod app;
mod backend;
mod network_identity;

#[derive(Parser, Debug)]
struct CliArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args, Debug)]
pub struct WalletArgs {
    #[clap(short, long, env)]
    wallet_private_key_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct ClusterArgs {
    #[clap(long, env, default_value_t = Cluster::Localnet.url().to_string())]
    rpc: String,

    #[clap(long, env, default_value_t = Cluster::Localnet.ws_url().to_string())]
    ws_rpc: String,
}

#[derive(Serialize, Deserialize, Zeroable)]
pub struct State {
    pub config: CoordinatorConfig<ClientId>,
    pub model: Model,
}

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    ShowStaticP2PIdentity {
        identity_secret_key_path: Option<PathBuf>,
    },
    CreateStaticP2PIdentity {
        save_path: PathBuf,
    },
    CreateRun {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,
    },
    CloseRun {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,
    },
    SetPaused {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(short, long, env)]
        resume: bool,
    },
    UpdateConfig {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        config_path: PathBuf,
    },
    Train {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(flatten)]
        args: TrainArgs,

        #[clap(long, env)]
        ticker: bool,
    },
}

impl From<ClusterArgs> for Cluster {
    fn from(val: ClusterArgs) -> Self {
        let rpc = val.rpc.trim_matches('"').to_string();
        let ws_rpc = val.ws_rpc.trim_matches('"').to_string();
        Cluster::Custom(rpc, ws_rpc)
    }
}

impl TryInto<Keypair> for WalletArgs {
    type Error = anyhow::Error;

    fn try_into(self) -> std::result::Result<Keypair, Self::Error> {
        let wallet_keypair = match std::env::var("RAW_WALLET_PRIVATE_KEY").ok() {
            Some(raw_wallet_private_key) => Keypair::from_base58_string(&raw_wallet_private_key),
            None => match self.wallet_private_key_path {
                Some(wallet_private_key_path) => match Keypair::read_from_file(wallet_private_key_path) {
                    Ok(wallet_keypair) => wallet_keypair,
                    Err(err) => bail!("{}", err),
                },
                None => bail!("No wallet private key! Must pass --wallet-private-key-path or set RAW_WALLET_PRIVATE_KEY")
            }
        };

        Ok(wallet_keypair)
    }
}

async fn async_main() -> Result<()> {
    let args = CliArgs::parse();

    match args.command {
        Commands::ShowStaticP2PIdentity {
            identity_secret_key_path,
        } => print_identity_keys(identity_secret_key_path.as_ref()),
        Commands::CreateStaticP2PIdentity { save_path } => {
            let identity_secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
            std::fs::write(&save_path, identity_secret_key.secret().as_bytes())?;
            print_identity_keys(Some(&save_path))?;
            println!("Wrote secret key to {}", save_path.display());
            Ok(())
        }
        Commands::CreateRun {
            cluster,
            wallet,
            run_id,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let created = backend.create_run(run_id.clone()).await?;
            let locked = backend.get_balance(&created.account).await?;
            println!(
                "Created run {} with transaction {}",
                run_id, created.transaction
            );
            println!("Instance account: {}", created.instance);
            println!("Coordinator account: {}", created.account);
            println!("Locked for storage: {:.9} SOL", lamports_to_sol(locked));
            Ok(())
        }
        Commands::CloseRun {
            cluster,
            wallet,
            run_id,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let balance = backend.get_balance(&key_pair.pubkey()).await?;
            let (instance_pda, instance) = backend.get_coordinator_instance(&run_id).await?;
            let closed = backend.close_run(instance_pda, instance.account).await?;
            println!("Closed run {} with transaction {}", run_id, closed);
            let recovered = backend.get_balance(&key_pair.pubkey()).await? - balance;
            println!("Recovered {:.9} SOL", lamports_to_sol(recovered));
            Ok(())
        }
        Commands::UpdateConfig {
            cluster,
            wallet,
            run_id,
            config_path,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let state: State = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path)
                    .with_context(|| format!("failed to read config toml file {config_path:?}"))?,
            )?)
            .with_context(|| format!("failed to parse config toml file {config_path:?}"))?;
            let (instance_pda, instance) = backend.get_coordinator_instance(&run_id).await?;
            let set = backend
                .update_config_and_model(
                    instance_pda,
                    instance.account,
                    Some(state.config),
                    Some(state.model),
                )
                .await?;
            println!("Updated config of {} with transaction {}", run_id, set);
            Ok(())
        }
        Commands::SetPaused {
            cluster,
            wallet,
            run_id,
            resume,
        } => {
            let run_id = run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let paused = !resume;
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(
                cluster.into(),
                key_pair.clone(),
                CommitmentConfig::confirmed(),
            )
            .unwrap();
            let (instance_pda, instance) = backend.get_coordinator_instance(&run_id).await?;
            let set = backend
                .set_paused(instance_pda, instance.account, paused)
                .await?;
            println!(
                "Set pause state to {} on run {} with transaction {}",
                paused, run_id, set
            );
            Ok(())
        }
        Commands::Train {
            cluster,
            wallet,
            args,
            ticker,
        } => {
            exercise_sdpa_if_needed();

            let hub_read_token = std::env::var("HF_TOKEN").ok();
            let checkpoint_upload_info = args.checkpoint_config()?;
            let eval_tasks = args.eval_tasks()?;

            psyche_tui::init_logging(args.logs, Level::INFO, args.write_log.clone());

            info!(
                "============ Client Startup at {} ============",
                OffsetDateTime::now_utc()
            );

            let run_id = args.run_id.trim_matches('"').to_string(); // Trim quotes, if any
            let identity_secret_key: SecretKey =
                read_identity_secret_key(args.identity_secret_key_path.as_ref())?
                    .unwrap_or_else(|| SecretKey::generate(&mut rand::rngs::OsRng));

            let wallet_keypair: Arc<Keypair> = Arc::new(wallet.try_into()?);

            let wandb_info = args.wandb_info(format!("{}-{}", run_id, wallet_keypair.pubkey()))?;

            let (cancel, tx_tui_state) = maybe_start_render_loop(
                (args.logs == LogOutput::TUI).then(|| Tabs::new(Default::default(), &TAB_NAMES)),
            )?;

            let (mut app, allowlist, p2p, state_options) = AppBuilder::new(AppParams {
                cancel,
                tx_tui_state,
                identity_secret_key,
                wallet_keypair,
                cluster: cluster.into(),
                ticker,
                run_id,
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
                max_concurrent_parameter_requests: args.max_concurrent_parameter_requests,
            })
            .build()
            .await
            .unwrap();

            app.run(allowlist, p2p, state_options).await
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
