use anyhow::Result;
use app::App;
use clap::{ArgAction, Parser};
use psyche_coordinator::Coordinator;
use psyche_tui::LogOutput;
use tracing::{info, Level};

mod app;
mod dashboard;

#[derive(Parser, Debug)]
struct Args {
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
    state: Option<String>,

    /// Path to TOML of data server config
    #[clap(long)]
    data_config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    psyche_tui::init_logging(
        if args.tui {
            LogOutput::TUI
        } else {
            LogOutput::Console
        },
        Level::INFO,
    );

    let coordinator = match args.state {
        Some(state_path) => toml::from_str(std::str::from_utf8(&std::fs::read(state_path)?)?)?,
        None => Coordinator::default(),
    };

    info!("joining gossip room");

    let data_server_config = match args.data_config {
        Some(config_path) => Some(toml::from_str(std::str::from_utf8(&std::fs::read(
            config_path,
        )?)?)?),
        None => None,
    };

    App::new(
        args.tui,
        coordinator,
        data_server_config,
        args.p2p_port,
        args.server_port,
    )
    .await?
    .run()
    .await?;

    Ok(())
}
