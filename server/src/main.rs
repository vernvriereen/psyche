use crate::app::App;
use crate::protocol::NC;
use crate::tui::TUI;

use anyhow::{bail, Result};
use clap::{ArgAction, Parser};
use iroh::net::relay::{RelayMap, RelayMode, RelayUrl};
use psyche_network::PeerList;
use psyche_tui::LogOutput;
use std::{str::FromStr, sync::mpsc, thread, time::Duration};
use tokio::time::{interval, interval_at, Instant};
use tracing::info;

mod app;
mod protocol;
mod tui;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,
    #[clap(short, long)]
    relay: Option<RelayUrl>,
    #[clap(long)]
    no_relay: bool,

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

    peer_list: Option<String>,

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

    let PeerList(peers) = args
        .peer_list
        .map(|p| PeerList::from_str(&p).unwrap())
        .unwrap_or_default();

    info!("joining gossip room");

    let secret_key = args.secret_key.map(|k| k.parse().unwrap());

    let relay_mode = match (args.no_relay, args.relay) {
        (false, None) => RelayMode::Default,
        (false, Some(url)) => RelayMode::Custom(RelayMap::from_url(url)),
        (true, None) => RelayMode::Disabled,
        (true, Some(_)) => bail!("You cannot set --no-relay and --relay at the same time"),
    };
    info!("using relay servers: {:?}", &relay_mode);

    let network = NC::init(&args.run_id, args.bind_port, relay_mode, peers, secret_key).await?;

    let tui = args.tui;

    let tx_state = if tui {
        psyche_tui::start_render_loop::<TUI>().unwrap()
    } else {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for item in rx {
                info!("{:?}", item);
            }
        });
        tx
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
    .await;

    Ok(())
}
