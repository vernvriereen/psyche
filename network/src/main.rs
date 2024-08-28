use anyhow::{bail, Error, Result};
use bytes::Bytes;
use clap::{ArgAction, Parser};
use download_manager::{Download, DownloadManager, DownloadUpdate};
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
    time::{Duration, Instant},
};
use tokio::select;
use tokio::time::interval;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};
use tui::UIState;
use util::fmt_relay_mode;

mod download_manager;
mod peer_list;
mod state;
mod tui;
mod util;

const BANDWIDTH_GRAPH_SIZE: usize = 60;

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

const GOSSIP_TOPIC: &str = "psyche gossip";

fn gossip_topic() -> TopicId {
    let mut hasher = Sha256::new();
    hasher.update(GOSSIP_TOPIC);
    let result = hasher.finalize();
    TopicId::from_bytes(result.into())
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
    info!("joining chat room");

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

    let node = Node::memory()
        .secret_key(secret_key)
        .relay_mode(relay_mode)
        .bind_port(args.bind_port)
        .spawn()
        .await?;

    info!("our node id: {}", node.node_id());

    let ticket = {
        let me = node.endpoint().node_addr().await?;
        let peers = peers.iter().cloned().chain([me]).collect();
        PeerList(peers)
    };

    info!("ticket to join us: {ticket}");

    let peer_ids: Vec<_> = peers.iter().map(|p| p.node_id).collect();
    if peers.is_empty() {
        info!("waiting for peers to join us...");
    } else {
        info!("trying to connect to {} peers...", peers.len());
        // add the peer addrs from the ticket to our endpoint's addressbook so that they can be dialed
        for peer in peers.into_iter() {
            node.net().add_node_addr(peer).await?;
        }
    };
    let (mut gossip_tx, mut gossip_rx) = node.gossip().subscribe(gossip_topic(), peer_ids).await?;
    info!("connected!");

    let mut send_data_interval = interval(Duration::from_secs(10));

    // if this is not 1s, the bandwidth chart will be wrong.
    let mut update_stats_interval = interval(Duration::from_secs(1));

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

    let mut state = State::new(15);
    let (mut tx_new_download, rx_new_download) = tokio::sync::mpsc::channel(100);

    let mut manager = DownloadManager::new(rx_new_download);

    loop {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = gossip_rx.next() => {
                on_gossip_event(&node, event, &mut tx_new_download).await?;
            }
            Some(update) = manager.poll_next() => {
                on_download_update(&mut state, update);
            }
            _ = send_data_interval.tick() => {
                on_tick(&node, &mut gossip_tx).await?;
            }
            _ = update_stats_interval.tick() => {
                on_update_stats(&node, &mut state, &mut tx_state).await?;
            }
            else => break,
        }
    }

    Ok(())
}

fn on_download_update(state: &mut State, update: DownloadUpdate) {
    state
        .bandwidth_tracker
        .add_event(update.downloaded_size_delta);
    state.last_seen.insert(update.from, Instant::now());

    let is_done = update.downloaded_size == update.total_size;
    if is_done {
        state.download_progesses.remove(&update.hash);
    } else {
        state.download_progesses.insert(update.hash.clone(), update);
    }
}
async fn on_update_stats(
    node: &MemNode,
    stats: &mut State,
    tx_state: &mut Sender<UIState>,
) -> Result<()> {
    let ticket = {
        let me = node.endpoint().node_addr().await?;
        PeerList(vec![me])
    };

    stats.join_ticket = ticket;

    for (peer_id, last_recvd) in node
        .endpoint()
        .remote_info_iter()
        .filter_map(|i| i.last_received().map(|r| (i.node_id, r)))
    {
        // after 2 minutes with no comms, assume a client is disconnected.
        if last_recvd.as_secs() < 120 {
            stats
                .last_seen
                .insert(peer_id, Instant::now().sub(last_recvd));
        } else {
            stats.last_seen.remove(&peer_id);
        }
    }

    stats
        .bandwidth_history
        .push_back(stats.bandwidth_tracker.get_bandwidth());

    if stats.bandwidth_history.len() > BANDWIDTH_GRAPH_SIZE {
        stats.bandwidth_history.pop_front();
    }

    let ui_state: UIState = (&*stats).into();
    tx_state.send(ui_state)?;

    Ok(())
}

async fn on_gossip_event(
    node: &MemNode,
    event: Result<Event>,
    tx_new_download: &mut tokio::sync::mpsc::Sender<Download>,
) -> Result<()> {
    if let Ok(Event::Gossip(GossipEvent::Received(msg))) = event {
        if let Ok((from, message)) = SignedMessage::verify_and_decode(&msg.content) {
            let name = from.fmt_short();
            match message {
                Message::DistroResult { blob_ticket } => {
                    info!(
                        "got blob ticket {} from {name}, downloading...",
                        blob_ticket.hash()
                    );

                    let progress = node
                        .blobs()
                        .download(blob_ticket.hash(), blob_ticket.node_addr().clone())
                        .await?;

                    tx_new_download
                        .send(Download::new(from, blob_ticket, progress))
                        .await
                        .unwrap();
                }
                Message::Message { text } => {
                    info!("{name}: {text}");
                }
            }
        }
    }

    Ok(())
}

async fn on_tick(
    node: &MemNode,
    sender: &mut (dyn Sink<Command, Error = Error> + Unpin),
) -> Result<()> {
    const DATA_SIZE_MB: usize = 10;
    let mut data = vec![0u8; DATA_SIZE_MB * 1024 * 1024];
    rand::thread_rng().fill(&mut data[..]);
    let blob_res = node.blobs().add_bytes(data).await?;
    let ticket = node
        .blobs()
        .share(blob_res.hash, blob_res.format, Default::default())
        .await?;

    let message = Message::DistroResult {
        blob_ticket: ticket,
    };

    let encoded_message = SignedMessage::sign_and_encode(node.endpoint().secret_key(), &message)?;
    if let Err(e) = sender.send(Command::Broadcast(encoded_message)).await {
        error!("Error sending message: {}", e);
    } else {
        info!("sent blob with hash {}", blob_res.hash);
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct SignedMessage {
    from: PublicKey,
    data: Bytes,
    signature: Signature,
}

impl SignedMessage {
    pub fn verify_and_decode(bytes: &[u8]) -> Result<(PublicKey, Message)> {
        let signed_message: Self = postcard::from_bytes(bytes)?;
        let key: PublicKey = signed_message.from;
        key.verify(&signed_message.data, &signed_message.signature)?;
        let message: Message = postcard::from_bytes(&signed_message.data)?;
        Ok((signed_message.from, message))
    }

    pub fn sign_and_encode(secret_key: &SecretKey, message: &Message) -> Result<Bytes> {
        let data: Bytes = postcard::to_stdvec(&message)?.into();
        let signature = secret_key.sign(&data);
        let from: PublicKey = secret_key.public();
        let signed_message = Self {
            from,
            data,
            signature,
        };
        let encoded = postcard::to_stdvec(&signed_message)?;
        Ok(encoded.into())
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    Message { text: String },
    DistroResult { blob_ticket: BlobTicket },
}
