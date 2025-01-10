mod app;
mod dashboard;

use anyhow::{bail, Context, Result};
use app::{App, DataServerInfo};
use clap::{ArgAction, Parser};
use psyche_centralized_shared::ClientId;
use psyche_coordinator::Coordinator;
use psyche_tui::LogOutput;
use std::path::{Path, PathBuf};
use tracing::{error, info, Level};

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Parser, Debug)]
enum Commands {
    ValidateConfig,
    Run,
}

#[derive(Parser, Debug, Clone)]
struct CommonArgs {
    /// Path to TOML of Coordinator state
    #[clap(long)]
    state: PathBuf,

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

    /// Path to TOML of data server config
    #[clap(long)]
    data_config: Option<PathBuf>,

    #[clap(long)]
    save_state_dir: Option<PathBuf>,

    #[clap(long)]
    init_warmup_time: Option<u64>,

    #[clap(long)]
    init_min_clients: Option<u16>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    withdraw_on_disconnect: bool,
}

fn load_config_state(
    common_args: CommonArgs,
) -> Result<(Coordinator<ClientId>, Option<DataServerInfo>)> {
    let coordinator: Coordinator<ClientId> = toml::from_str(std::str::from_utf8(
        &std::fs::read(&common_args.state).with_context(|| {
            format!(
                "failed to read coordinator state toml file {:?}",
                common_args.state
            )
        })?,
    )?)
    .with_context(|| {
        format!(
            "failed to parse coordinator state toml file {:?}",
            common_args.state
        )
    })?;

    if coordinator.config.cooldown_time == 0 && coordinator.config.checkpointers.is_empty() {
        bail!("cooldown time of 0 and no checkpointers will run forever. invalid coordinator state toml.")
    }

    let data_server_config = match common_args.data_config {
        Some(config_path) => {
            let mut data_config: DataServerInfo = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path).with_context(|| {
                    format!("failed to read data server config toml file {config_path:?}")
                })?,
            )?)
            .with_context(|| {
                format!("failed to parse data server config toml file {config_path:?}")
            })?;

            // data dir, if relative, should be relative to the config's path.
            if !data_config.dir.is_absolute() {
                let config_dir = Path::new(&config_path).parent().unwrap_or(Path::new(""));
                data_config.dir = config_dir.join(data_config.dir);
            }
            Some(data_config)
        }
        None => None,
    };

    Ok((coordinator, data_server_config))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let common_args = args.common;
    let command = args.command;

    let config = load_config_state(common_args.clone());
    match command {
        Commands::ValidateConfig => {
            psyche_tui::init_logging(LogOutput::Console, Level::INFO, None);
            match config {
                Ok(_) => info!("Configs are OK!"),
                Err(error) => error!("Error found in config: {}", error),
            }
        }
        Commands::Run => {
            psyche_tui::init_logging(
                if common_args.tui {
                    LogOutput::TUI
                } else {
                    LogOutput::Console
                },
                Level::INFO,
                None,
            );
            match config {
                Ok(config) => {
                    App::new(
                        common_args.tui,
                        config.0,
                        config.1,
                        common_args.server_port,
                        common_args.save_state_dir,
                        common_args.init_warmup_time,
                        common_args.init_min_clients,
                        common_args.withdraw_on_disconnect,
                    )
                    .await?
                    .run()
                    .await?
                }
                Err(error) => error!("Error found in config: {}", error),
            }
        }
    }

    Ok(())
}
