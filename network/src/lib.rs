use anyhow::{bail, Error, Result};
use bytes::Bytes;
use chrono::{Local, Timelike};
use clap::{ArgAction, Parser};
use download_manager::{DownloadManager, DownloadUpdate};
use ed25519_dalek::Signature;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use iroh::{
    base::ticket::BlobTicket,
    gossip::{
        net::{Command, Event, GossipEvent},
        proto::TopicId,
    },
    net::{
        key::{PublicKey, SecretKey},
        relay::{RelayMap, RelayMode, RelayUrl},
        NodeAddr,
    },
    node::{MemNode, Node},
};
use peer_list::PeerList;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use state::State;
use std::{
    marker::PhantomData,
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
use util::{fmt_relay_mode, gossip_topic};

mod download_manager;
mod peer_list;
mod state;
mod util;

const BANDWIDTH_GRAPH_SIZE: usize = 60;

const GOSSIP_TOPIC: &str = "psyche gossip";

type State = 

pub struct PsycheNetworkConnection<Message, BroadcastMessage, State> {
    node: MemNode,
    state: State,
    _message: PhantomData<Message>,
    _broadcast_message: PhantomData<BroadcastMessage>,
    gossip_tx: Box<dyn Sink<Command, Error = Error>>,
    gossip_rx: Box<dyn Stream<Item = std::result::Result<Event, Error>>>,
}

impl<Message, BroadcastMessage> PsycheNetworkConnection<Message, BroadcastMessage> {
    async fn init(
        run_id: &str,
        port: Option<u16>,
        relay_mode: RelayMode,
        bootstrap_peers: Vec<NodeAddr>,
        secret_key: Option<SecretKey>,
    ) -> Result<Self> {
        let secret_key = match secret_key {
            None => SecretKey::generate(),
            Some(key) => key,
        };
        info!("our secret key: {secret_key}");

        info!("using relay servers: {}", fmt_relay_mode(&relay_mode));

        let node = Node::memory()
            .secret_key(secret_key)
            .relay_mode(relay_mode)
            .bind_port(port.unwrap_or(0))
            .spawn()
            .await?;

        info!("our node id: {}", node.node_id());

        let peer_ids: Vec<_> = bootstrap_peers.iter().map(|p| p.node_id).collect();
        if bootstrap_peers.is_empty() {
            info!("waiting for peers to join us...");
        } else {
            info!("trying to connect to {} peers...", bootstrap_peers.len());
            // add the peer addrs from the ticket to our endpoint's addressbook so that they can be dialed
            for peer in bootstrap_peers.into_iter() {
                node.net().add_node_addr(peer).await?;
            }
        };
        let (gossip_tx, gossip_rx) = node
            .gossip()
            .subscribe(gossip_topic(run_id), peer_ids)
            .await?;
        info!("connected!");

        Ok(Self {
            node,
            gossip_tx: Box::new(gossip_tx),
            gossip_rx: Box::new(gossip_rx),
            state: State::new(15),
            _message: PhantomData::<Message>,
            _broadcast_message: PhantomData::<BroadcastMessage>,
        })
    }

    fn send(peer: PublicKey, message: Message) {}
    fn broadcast(message: BroadcastMessage) {}
}

pub enum PsycheNetworkUpdate {
    DownloadComplete(DistroResult),
}

#[tokio::main]
async fn main() -> Result<()> {
    // fire at wall-clock 15-second intervals.
    let mut send_data_interval = {
        let now = Local::now();
        let seconds_until_next: u64 = 15 - (now.second() as u64 % 15);
        let start = Instant::now() + Duration::from_secs(seconds_until_next as u64);
        interval_at(start.into(), Duration::from_secs(15))
    };

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

    let mut manager = DownloadManager::default();

    loop {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = gossip_rx.next() => {
                on_gossip_event(&node, &mut state, event, &mut manager).await?;
            }
            Some(update) = manager.poll_next() => {
                on_download_update(&mut state, update);
            }
            _ = send_data_interval.tick() => {
                on_tick(&node, &mut state, &mut gossip_tx).await?;
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
    state: &mut State,
    event: Result<Event>,
    download_manager: &mut DownloadManager,
) -> Result<()> {
    if let Ok(Event::Gossip(GossipEvent::Received(msg))) = event {
        if let Ok((from, message)) = SignedMessage::verify_and_decode(&msg.content) {
            let name = from.fmt_short();
            match message {
                Message::DistroResult { blob_ticket, step } => {
                    if step != state.current_step {
                        warn!(
                            "got a blob from {name} but its step {step} != {}, the current step.",
                            state.current_step
                        );
                        return Ok(());
                    }
                    info!(
                        "got blob ticket {} from {name}, downloading...",
                        blob_ticket.hash()
                    );

                    let progress = node
                        .blobs()
                        .download(blob_ticket.hash(), blob_ticket.node_addr().clone())
                        .await?;

                    download_manager.add(from, blob_ticket, progress);
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
    DistroResult { blob_ticket: BlobTicket, step: u64 },
}
