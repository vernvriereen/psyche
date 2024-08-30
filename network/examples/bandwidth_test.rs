use anyhow::{bail, Error, Result};
use bytes::Bytes;
use chrono::{Local, Timelike};
use clap::{ArgAction, Parser};
use download_manager::{DownloadManager, DownloadUpdate};
use ed25519_dalek::Signature;
use futures_util::{Sink, SinkExt, StreamExt};
use iroh::{
    base::ticket::BlobTicket,
    gossip::{
        net::{Command, Event, GossipEvent},
        proto::TopicId,
    },
    net::{
        key::{PublicKey, SecretKey},
        relay::{RelayMap, RelayMode, RelayUrl},
    },
    node::{MemNode, Node},
};
use peer_list::PeerList;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use state::State;
use std::{
    ops::Sub,
    str::FromStr,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::time::interval;
use tokio::{select, time::interval_at};
use tracing::{error, info, warn, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};
use tui::UIState;
use util::fmt_relay_mode;

use crate::create_psyche_network_connection;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,
    #[clap(short, long)]
    relay: Option<RelayUrl>,
    #[clap(long)]
    no_relay: bool,

    #[clap(short, long, default_value = "0")]
    bind_port: u16,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.tui {
        let subscriber = tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(Level::INFO.into())
                    .from_env_lossy(),
            )
            .with(tui_logger::tracing_subscriber_layer());

        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global default subscriber");
    } else {
        let subscriber = tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(Level::INFO.into())
                    .from_env_lossy(),
            )
            .with(fmt::layer().with_writer(std::io::stdout));
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global default subscriber");
    };

    let PeerList(peers) = args
        .peer_list
        .map(|p| PeerList::from_str(&p).unwrap())
        .unwrap_or_default();

    info!("joining gossip room");

    let secret_key = match args.secret_key {
        None => SecretKey::generate(),
        Some(key) => key.parse()?,
    };
    info!("our secret key: {secret_key}");

    let relay_mode = match (args.no_relay, args.relay) {
        (false, None) => RelayMode::Default,
        (false, Some(url)) => RelayMode::Custom(RelayMap::from_url(url)),
        (true, None) => RelayMode::Disabled,
        (true, Some(_)) => bail!("You cannot set --no-relay and --relay at the same time"),
    };
    info!("using relay servers: {}", fmt_relay_mode(&relay_mode));

    let network =
        create_psyche_network_connection(peers, secret_key, relay_mode, args.bind_port).await?;

    let ticket = network.join_ticket();
    info!("ticket to join us: {ticket}");

    // fire at wall-clock 15-second intervals.
    let mut send_data_interval = {
        let now = Local::now();
        let seconds_until_next: u64 = 15 - (now.second() as u64 % 15);
        let start = Instant::now() + Duration::from_secs(seconds_until_next as u64);
        interval_at(start.into(), Duration::from_secs(15))
    };

    let (mut tx_state, rx_state) = mpsc::channel();
    let tui = args.tui;
    thread::spawn(move || {
        if tui {
            tui::start_render_loop(rx_state).unwrap();
        } else {
            for item in rx_state {
                info!("{:?}", item);
            }
        }
    });

    loop {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = network.poll_next() => {
                match event {
                    PsycheNetworkUpdate::DownloadComplete(distro_result) => {
                        info!("Download complete: step {}, {} bytes.", distro_result.step, distro_result.result.len());
                    }
                }
            }
            _ = send_data_interval.tick() => {
                on_tick(&node, &mut state, &mut gossip_tx).await?;
            }
            else => break,
        }
    }

    Ok(())
}

async fn on_tick(
    node: &MemNode,
    state: &mut State,
    sender: &mut (dyn Sink<Command, Error = Error> + Unpin),
) -> Result<()> {
    const DATA_SIZE_MB: usize = 10;
    let mut data = vec![0u8; DATA_SIZE_MB * 1024 * 1024];
    rand::thread_rng().fill(&mut data[..]);
    let blob_res = node.blobs().add_bytes(data).await?;
    let blob_ticket = node
        .blobs()
        .share(blob_res.hash, blob_res.format, Default::default())
        .await?;

    let unix_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went forwads :)");
    let step = (unix_time.as_secs() + 2) / 15;
    info!("new step {step}");
    if step != state.current_step + 1 {
        warn!(
            "new step {step} is not 1 greater than prev step {}",
            state.current_step + 1
        );
    }

    state.current_step = step;

    state
        .currently_sharing_blobs
        .push((step, blob_ticket.clone()));

    // keep shorter than 5 pl0x
    state
        .currently_sharing_blobs
        .drain(0..(state.currently_sharing_blobs.len().saturating_sub(5)));

    let message = Message::DistroResult { step, blob_ticket };

    let encoded_message = SignedMessage::sign_and_encode(node.endpoint().secret_key(), &message)?;
    if let Err(e) = sender.send(Command::Broadcast(encoded_message)).await {
        error!("Error sending message: {}", e);
    } else {
        info!("broadcasted blob hash for step {step}: {}", blob_res.hash);
    }

    Ok(())
}
