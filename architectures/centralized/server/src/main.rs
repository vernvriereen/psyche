mod app;
mod dashboard;

use anyhow::{bail, Context, Result};
use app::{App, DataServerInfo};
use bytemuck::Zeroable;
use clap::{ArgAction, Parser};
use psyche_centralized_shared::ClientId;
use psyche_coordinator::Coordinator;
use psyche_tui::LogOutput;
use std::path::{Path, PathBuf};
use tracing::{info, Level};

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Parser, Debug)]
enum Commands {
    ValidateConfig,
}

#[derive(Parser, Debug, Clone)]
struct CommonArgs {
    /// if not specified, a random free port will be chosen.
    #[clap(short, long)]
    p2p_port: Option<u16>,

    /// if not specified, a random free port will be chosen.
    #[clap(short, long)]
    server_port: Option<u16>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    tui: bool,

    /// Path to TOML of Coordinator state
    #[clap(long)]
    state: Option<PathBuf>,

    /// Path to TOML of data server config
    #[clap(long)]
    data_config: Option<PathBuf>,

    #[clap(long)]
    save_state_dir: Option<PathBuf>,

    #[clap(long)]
    init_warmup_time: Option<u64>,

    #[clap(long)]
    init_min_clients: Option<u32>,
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let common_args = args.common;
    let command = args.command;
    psyche_tui::init_logging(
        if common_args.tui {
            LogOutput::TUI
        } else {
            LogOutput::Console
        },
        Level::INFO,
        None,
    );

    let coordinator: Coordinator<ClientId> = match common_args.state {
        Some(state_path) => toml::from_str(std::str::from_utf8(&std::fs::read(&state_path)?)?)
            .with_context(|| {
                format!("failed to parse coordinator state toml file {state_path:?}")
            })?,
        None => Coordinator::<ClientId>::zeroed(),
    };

    if coordinator.config.cooldown_time == 0 && coordinator.config.checkpointers.is_empty() {
        bail!("cooldown time of 0 and no checkpointers will run forever. invalid coordinator state toml.")
    }

    let data_server_config = match common_args.data_config {
        Some(config_path) => {
            let mut data_config: DataServerInfo =
                toml::from_str(std::str::from_utf8(&std::fs::read(&config_path)?)?).with_context(
                    || format!("failed to parse data server config toml file {config_path:?}"),
                )?;

            // data dir, if relative, should be relative to the config's path.
            if !data_config.dir.is_absolute() {
                let config_dir = Path::new(&config_path).parent().unwrap_or(Path::new(""));
                data_config.dir = config_dir.join(data_config.dir);
            }
            Some(data_config)
        }
        None => None,
    };

    match command {
        Some(Commands::ValidateConfig) => {
            info!("configs are OK!");
        }
        None => {
            App::new(
                common_args.tui,
                coordinator,
                data_server_config,
                common_args.p2p_port,
                common_args.server_port,
                common_args.save_state_dir,
                common_args.init_warmup_time,
                common_args.init_min_clients,
            )
            .await?
            .run()
            .await?;
        }
    }

    Ok(())
}
