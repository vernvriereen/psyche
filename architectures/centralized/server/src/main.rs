use crate::app::App;

use anyhow::Result;
use app::{Tabs, TAB_NAMES};
use clap::{ArgAction, Parser};
use psyche_centralized_shared::NC;
use psyche_network::RelayMode;
use psyche_tui::LogOutput;
use std::time::Duration;
use tokio::time::{interval, interval_at, Instant};
use tracing::info;

mod app;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,

    #[clap(short, long)]
    bind_port: Option<u16>,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    psyche_tui::init_logging(if args.tui {
        LogOutput::TUI
    } else {
        LogOutput::Console
    });

    info!("joining gossip room");

    let secret_key = args.secret_key.map(|k| k.parse().unwrap());

    let network = NC::init(
        &args.run_id,
        args.bind_port,
        RelayMode::Default,
        vec![],
        secret_key,
    )
    .await?;

    let tui = args.tui;

    let tx_state = match tui {
        true => Some(psyche_tui::start_render_loop(Tabs::new(
            Default::default(),
            &TAB_NAMES,
        ))?),
        false => None,
    };

    // tick every second
    let tick_interval = {
        let duration = Duration::from_secs(1);
        interval_at(Instant::now() + duration, duration)
    };

    App::new(
        network,
        tx_state,
        tick_interval,
        interval(Duration::from_millis(150)),
    )
    .run()
    .await?;

    Ok(())
}
