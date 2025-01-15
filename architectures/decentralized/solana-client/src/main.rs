use crate::{
    app::{AppBuilder, AppParams},
    backend::SolanaBackend,
};

use anchor_client::{
    solana_sdk::{
        pubkey::Pubkey,
        signature::{EncodableKey, Keypair},
        signer::Signer,
    },
    Cluster,
};
use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use psyche_client::{
    exercise_sdpa_if_needed, print_identity_keys, read_identity_secret_key, TrainArgs,
};
use psyche_coordinator::{model::Model, CoordinatorConfig};
use psyche_network::{PublicKey, SecretKey};
use psyche_solana_coordinator::ClientId;
use psyche_tui::LogOutput;
use serde::Deserialize;
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

#[allow(clippy::large_enum_variant)] // it's only used at startup, we don't care.
#[derive(Subcommand, Debug)]
enum Commands {
    ShowIdentity {
        identity_secret_key_path: Option<PathBuf>,
    },
    CreateIdentity {
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
    SetWhitelist {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(long, env)]
        members_path: PathBuf,
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
        config_path: Option<PathBuf>,

        #[clap(long, env)]
        model_path: Option<PathBuf>,
    },
    JoinRun {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(short, long, env)]
        run_id: String,

        #[clap(flatten)]
        identity: Identity,
    },
    Train {
        #[clap(flatten)]
        cluster: ClusterArgs,

        #[clap(flatten)]
        wallet: WalletArgs,

        #[clap(flatten)]
        args: TrainArgs,
    },
}

impl From<ClusterArgs> for Cluster {
    fn from(val: ClusterArgs) -> Self {
        Cluster::Custom(val.rpc, val.ws_rpc)
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

#[derive(Args, Clone, Debug, Deserialize)]
struct Identity {
    #[clap(long, env)]
    signer: Pubkey,

    #[clap(long, env)]
    p2p_identity: PublicKey,
}

impl From<Identity> for ClientId {
    fn from(val: Identity) -> Self {
        ClientId::new(val.signer, *val.p2p_identity.as_bytes())
    }
}

async fn async_main() -> Result<()> {
    let args = CliArgs::parse();

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
        Commands::CreateRun {
            cluster,
            wallet,
            run_id,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(cluster.into(), key_pair.clone()).unwrap();
            let created = backend.create_run(run_id.clone()).await?;
            println!(
                "Created run {} with transaction {}!",
                run_id, created.transaction
            );
            println!("Instance account: {}", created.instance);
            println!("Coordinator account: {}", created.account);
            Ok(())
        }
        Commands::SetWhitelist {
            cluster,
            wallet,
            run_id,
            members_path,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(cluster.into(), key_pair.clone()).unwrap();
            let members: Vec<Identity> = toml::from_str(std::str::from_utf8(
                &std::fs::read(&members_path).with_context(|| {
                    format!("failed to read whitelist members toml file {members_path:?}")
                })?,
            )?)
            .with_context(|| {
                format!("failed to parse whitelist members toml file {members_path:?}")
            })?;
            let num_members = members.len();
            let set = backend
                .set_whitelist(&run_id, members.into_iter().map(|x| x.into()).collect())
                .await?;
            println!(
                "Set whitelist of {} members on run {} with transaction {}",
                num_members, run_id, set
            );
            Ok(())
        }
        Commands::UpdateConfig {
            cluster,
            wallet,
            run_id,
            config_path,
            model_path,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(cluster.into(), key_pair.clone()).unwrap();
            let config: Option<CoordinatorConfig<ClientId>> = match config_path {
                Some(config_path) => Some(
                    toml::from_str(std::str::from_utf8(
                        &std::fs::read(&config_path).with_context(|| {
                            format!("failed to read coordinator config toml file {config_path:?}")
                        })?,
                    )?)
                    .with_context(|| {
                        format!("failed to parse coordinator config toml file {config_path:?}")
                    })?,
                ),
                None => None,
            };
            let model: Option<Model> = match model_path {
                Some(model_path) => Some(
                    toml::from_str(std::str::from_utf8(
                        &std::fs::read(&model_path).with_context(|| {
                            format!("failed to read model toml file {model_path:?}")
                        })?,
                    )?)
                    .with_context(|| format!("failed to parse model toml file {model_path:?}"))?,
                ),
                None => None,
            };
            let set = backend
                .update_config_and_model(&run_id, config, model)
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
            let paused = !resume;
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(cluster.into(), key_pair.clone()).unwrap();
            let set = backend.set_paused(&run_id, paused).await?;
            println!(
                "Set pause state to {} on run {} with transaction {}",
                paused, run_id, set
            );
            Ok(())
        }
        Commands::JoinRun {
            cluster,
            wallet,
            run_id,
            identity,
        } => {
            let key_pair: Arc<Keypair> = Arc::new(wallet.try_into()?);
            let backend = SolanaBackend::new(cluster.into(), key_pair.clone()).unwrap();
            let joined = backend.join_run(&run_id, identity.clone().into()).await?;
            println!(
                "Joined run {} from {} (signer {}) and p2p identity {} with transaction {}",
                run_id,
                key_pair.pubkey(),
                identity.signer,
                identity.p2p_identity,
                joined
            );
            Ok(())
        }
        Commands::Train {
            cluster,
            wallet,
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

            let wallet_keypair: Arc<Keypair> = Arc::new(wallet.try_into()?);

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
                cluster: cluster.into(),
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
