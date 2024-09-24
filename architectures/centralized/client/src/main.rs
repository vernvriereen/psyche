use crate::app::App;

use anyhow::Result;
use app::{Tabs, TAB_NAMES};
use clap::{ArgAction, Parser};
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::NC;
use psyche_network::{RelayMode, SecretKey, TcpClient};
use psyche_tui::{maybe_start_render_loop, LogOutput};
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

    #[clap(long, default_value_t = 1)]
    data_bid: u32,

    #[clap(long)]
    server_addr: String,
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

    let secret_key: SecretKey = args
        .secret_key
        .map(|k| k.parse().unwrap())
        .unwrap_or_else(SecretKey::generate);

    let tui = args.tui;

    let (cancel, tx_tui_state) =
        maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

    // tick every second
    let tick_interval = {
        let duration = Duration::from_secs(1);
        interval_at(Instant::now() + duration, duration)
    };

    let server_conn = TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
        &args.server_addr,
        secret_key.public().into(),
        secret_key.clone(),
    )
    .await?;

    App::new(
        cancel,
        secret_key.clone(),
        server_conn,
        tx_tui_state,
        tick_interval,
        interval(Duration::from_millis(150)),
        &args.run_id,
        args.data_bid,
    )
    .run(
        NC::init(
            &args.run_id,
            args.bind_port,
            RelayMode::Default,
            vec![],
            Some(secret_key),
        )
        .await?,
    )
    .await?;

    Ok(())
}
