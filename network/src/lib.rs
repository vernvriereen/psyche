use anyhow::{Error, Result};
use download_manager::{DownloadManager, DownloadUpdate};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use iroh::{
    base::ticket::BlobTicket,
    gossip::net::{Command, Event, GossipEvent},
    net::{
        key::{PublicKey, SecretKey},
        relay::RelayMode,
        NodeAddr,
    },
    node::{MemNode, Node},
};
use signed_message::SignedMessage;
use state::State;
use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::Sub,
    time::{Duration, Instant},
};
use tokio::select;
use tokio::time::{interval, Interval};
use tracing::info;
use util::{fmt_relay_mode, gossip_topic, Networkable};

mod download_manager;
mod peer_list;
mod signed_message;
mod state;
mod tui;
mod util;

pub use peer_list::PeerList;
pub use tui::{NetworkTUI, NetworkTUIState};

pub struct NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    node: MemNode,
    state: State,
    gossip_tx: Box<dyn Sink<Command, Error = Error> + Unpin>,
    gossip_rx: Box<dyn Stream<Item = std::result::Result<Event, Error>> + Unpin>,
    download_manager: DownloadManager,
    _broadcast_message: PhantomData<BroadcastMessage>,
    _download: PhantomData<Download>,
    update_stats_interval: Interval,
}

impl<B, D> Debug for NetworkConnection<B, D>
where
    B: Networkable,
    D: Networkable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConnection")
            .field("node", &self.node)
            .field("state", &self.state)
            .field("download_manager", &self.download_manager)
            .field("update_stats_interval", &self.update_stats_interval)
            .finish()
    }
}

impl<BroadcastMessage, Download> NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    pub async fn init(
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

        // TODO write a peer discovery service, and parameterize this impl based on it.
        // TODO add an allowlist of public keys, don't let any connections from people with keys not in that list.
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

        // if this is not 1s, the bandwidth chart will be wrong.
        let update_stats_interval = interval(Duration::from_secs(1));

        Ok(Self {
            node,
            gossip_tx: Box::new(gossip_tx),
            gossip_rx: Box::new(gossip_rx),
            update_stats_interval,
            state: State::new(15),
            download_manager: Default::default(),
            _broadcast_message: Default::default(),
            _download: Default::default(),
        })
    }

    pub async fn broadcast(&mut self, message: &BroadcastMessage) -> Result<()> {
        let encoded_message =
            SignedMessage::sign_and_encode(self.node.endpoint().secret_key(), message)?;
        self.gossip_tx
            .send(Command::Broadcast(encoded_message))
            .await
    }
    /// should this really await the whole download..?
    /// for now, you can get your result as a message from polling this connection.
    pub async fn download(&mut self, ticket: BlobTicket) -> Result<()> {
        let progress = self
            .node
            .blobs()
            .download(ticket.hash(), ticket.node_addr().clone())
            .await?;

        self.download_manager
            .add(ticket.node_addr().node_id, ticket, progress);

        // TODO write a future impl that resolves when the download manager has downloaded this future..?
        Ok(())
    }

    pub async fn add_downloadable(&mut self, data: Download) -> Result<BlobTicket> {
        let bytes = postcard::to_allocvec(&data)?;
        let blob_res = self.node.blobs().add_bytes(bytes).await?;
        let blob_ticket = self
            .node
            .blobs()
            .share(blob_res.hash, blob_res.format, Default::default())
            .await?;

        self.state.currently_sharing_blobs.push(blob_ticket.clone());

        Ok(blob_ticket)
    }

    pub async fn remove_downloadable(&self, ticket: BlobTicket) -> Result<()> {
        self.node.blobs().delete_blob(ticket.hash()).await
    }

    pub async fn join_ticket(&self) -> Result<String> {
        let me = self.node.endpoint().node_addr().await?;
        Ok(PeerList(vec![me]).to_string())
    }

    pub async fn poll_next(&mut self) -> Result<Option<NetworkEvent<BroadcastMessage, Download>>> {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = self.gossip_rx.next() => {
                if let Some(result) = parse_gossip_event(event) {
                    return Ok(Some(NetworkEvent::MessageReceived(result)));
                }
            }
            Some(update) = self.download_manager.poll_next() => {
                on_download_update(&mut self.state, update);
            }
              _ = self.update_stats_interval.tick() => {
                on_update_stats(&self.node, &mut self.state).await?;
            }
        }

        Ok(None)
    }
}

fn parse_gossip_event<BroadcastMessage: Networkable>(
    event: Result<Event>,
) -> Option<(PublicKey, BroadcastMessage)> {
    if let Ok(Event::Gossip(GossipEvent::Received(msg))) = event {
        if let Ok(result) = SignedMessage::<BroadcastMessage>::verify_and_decode(&msg.content) {
            return Some(result);
        }
    }

    None
}

pub enum NetworkEvent<BM, D>
where
    BM: Networkable,
    D: Networkable,
{
    MessageReceived((PublicKey, BM)),
    DownloadComplete(D),
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

async fn on_update_stats(node: &MemNode, stats: &mut State) -> Result<()> {
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

    const BANDWIDTH_GRAPH_SIZE: usize = 60;
    if stats.bandwidth_history.len() > BANDWIDTH_GRAPH_SIZE {
        stats.bandwidth_history.pop_front();
    }

    Ok(())
}
