use anyhow::{Error, Result};
use download_manager::{DownloadManager, DownloadManagerEvent, DownloadUpdate};
use futures_util::StreamExt;
use iroh::{endpoint::RemoteInfo, protocol::Router, NodeAddr};
use iroh_blobs::{net_protocol::Blobs, store::mem::Store, util::local_pool::LocalPool};
use iroh_gossip::net::{Gossip, GossipEvent, GossipReceiver, GossipSender};
use p2p_model_sharing::ParameterSharingMessage;
use state::State;
use std::{
    collections::HashSet,
    fmt::Debug,
    marker::PhantomData,
    net::{Ipv4Addr, SocketAddrV4},
    ops::Sub,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    select,
    sync::{mpsc::UnboundedReceiver, oneshot},
};
use tokio::{
    sync::mpsc,
    time::{interval, Interval},
};
use tracing::{debug, error, info, trace};
use util::{fmt_relay_mode, gossip_topic};

pub use ed25519::Signature;
pub use iroh::{NodeId, RelayMode};
pub use iroh_blobs::{ticket::BlobTicket, Hash};

mod download_manager;
mod networkable_node_identity;
mod p2p_model_sharing;
mod peer_list;
mod serde;
mod signed_message;
mod state;
mod tcp;
mod tui;
mod util;

pub use download_manager::{DownloadComplete, DownloadFailed};
pub use iroh::{Endpoint, PublicKey, SecretKey};
pub use networkable_node_identity::{FromSignedBytesError, NetworkableNodeIdentity};
pub use p2p_model_sharing::{ModelParameterSharing, ModelParameters, ALPN};
pub use peer_list::PeerList;
pub use serde::Networkable;
pub use signed_message::SignedMessage;
pub use tcp::{ClientNotification, TcpClient, TcpServer};
pub use tui::{NetworkTUIState, NetworkTui};

pub struct NetworkConnection<BroadcastMessage, Download>
where
    BroadcastMessage: Networkable,
    Download: Networkable,
{
    router: Arc<Router>,
    blobs_local_pool: LocalPool,
    blobs: Blobs<Store>,
    state: State,
    gossip_tx: GossipSender,
    gossip_rx: GossipReceiver,
    rx_model_parameter_req: UnboundedReceiver<ParameterSharingMessage>,
    download_manager: DownloadManager<Download>,
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
            .field("router", &self.router)
            .field("blobs_local_pool", &self.blobs_local_pool)
            .field("blobs", &self.blobs)
            .field("gossip_tx", &self.gossip_tx)
            .field("gossip_rx", &self.gossip_rx)
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
            None => SecretKey::generate(&mut rand::rngs::OsRng),
            Some(key) => key,
        };
        debug!("Using relay servers: {}", fmt_relay_mode(&relay_mode));

        // TODO add an allowlist of public keys, don't let any connections from people with keys not in that list.
        let endpoint = Endpoint::builder()
            .secret_key(secret_key)
            .relay_mode(relay_mode)
            .bind_addr_v4(SocketAddrV4::new(
                Ipv4Addr::new(0, 0, 0, 0),
                port.unwrap_or(0),
            ))
            .discovery_n0()
            .bind()
            .await?;

        let node_addr = endpoint.node_addr().await?;

        info!("Our node addr: {}", node_addr.node_id);

        let blobs_local_pool = LocalPool::default();
        let blobs = Blobs::memory().build(blobs_local_pool.handle(), &endpoint);

        let gossip = Gossip::builder().spawn(endpoint.clone()).await?;

        let (tx_model_parameter_req, rx_model_parameter_req) = mpsc::unbounded_channel();
        let model_parameter_sharing = ModelParameterSharing::new(tx_model_parameter_req);

        let router = Arc::new(
            Router::builder(endpoint)
                .accept(iroh_blobs::ALPN, blobs.clone())
                .accept(iroh_gossip::ALPN, gossip.clone())
                .accept(p2p_model_sharing::ALPN, model_parameter_sharing.clone())
                .spawn()
                .await?,
        );

        // add any bootstrap peers
        {
            let me = router.endpoint().node_addr().await?;
            let join_ticket = PeerList(vec![me]);
            info!("our join ticket: {}", join_ticket);
            if bootstrap_peers.is_empty() {
                info!("Waiting for peers to join us...");
            } else {
                info!("Trying to connect to {} peers...", bootstrap_peers.len());
                // add the peer addrs from the ticket to our endpoint's addressbook so that they can be dialed
                for peer in &bootstrap_peers {
                    router.endpoint().add_node_addr(peer.clone())?;
                }
            };
        }

        let (gossip_tx, gossip_rx) = gossip
            .subscribe(
                gossip_topic(run_id),
                bootstrap_peers.iter().map(|p| p.node_id).collect(),
            )?
            .split();
        info!("Connected!");

        // if this is not 1s, the bandwidth chart will be wrong.
        let update_stats_interval = interval(Duration::from_secs(1));

        Ok(Self {
            blobs_local_pool,
            blobs,
            gossip_rx,
            gossip_tx,
            rx_model_parameter_req,

            router,

            update_stats_interval,
            state: State::new(15),
            download_manager: DownloadManager::new()?,
            _broadcast_message: Default::default(),
            _download: Default::default(),
        })
    }

    pub async fn add_peers(&mut self, peers: Vec<NodeAddr>) -> Result<()> {
        peers
            .iter()
            .filter(|p| p.node_id != self.router.endpoint().node_id())
            .map(|peer| self.router.endpoint().add_node_addr(peer.clone()))
            .collect::<Result<Vec<_>>>()?;
        self.gossip_tx
            .join_peers(
                peers
                    .into_iter()
                    .map(|i| i.node_id)
                    .filter(|p| p != &self.router.endpoint().node_id())
                    .collect(),
            )
            .await?;
        Ok(())
    }

    pub async fn broadcast(&mut self, message: &BroadcastMessage) -> Result<()> {
        let encoded_message =
            SignedMessage::sign_and_encode(self.router.endpoint().secret_key(), message)?;
        self.gossip_tx.broadcast(encoded_message).await
    }

    pub async fn start_download(&mut self, ticket: BlobTicket) -> Result<()> {
        let mut progress = self
            .blobs
            .client()
            .download(ticket.hash(), ticket.node_addr().clone())
            .await?;
        self.state.currently_sharing_blobs.insert(ticket.hash());

        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                match progress.next().await {
                    None => break,
                    Some(val) => {
                        if let Err(err) = tx.send(val) {
                            panic!("Failed to send download progress: {err:?} {:?}", err.0);
                        }
                    }
                }
            }
        });

        self.download_manager.add(ticket, rx);

        Ok(())
    }

    pub async fn add_downloadable<N: Networkable>(&mut self, data: N) -> Result<BlobTicket> {
        let bytes = postcard::to_allocvec(&data)?;
        let blob_res = self.blobs.client().add_bytes(bytes).await?;
        let addr = self.router.endpoint().node_addr().await?;
        let blob_ticket = BlobTicket::new(addr, blob_res.hash, blob_res.format)?;

        trace!(
            "added downloadable hash {} size {}",
            blob_res.hash,
            blob_res.size
        );

        self.state
            .currently_sharing_blobs
            .insert(blob_ticket.hash());

        Ok(blob_ticket)
    }

    pub async fn remove_downloadable(&mut self, hash: iroh_blobs::Hash) -> Result<()> {
        self.blobs.client().delete_blob(hash).await?;
        self.state.currently_sharing_blobs.remove(&hash);
        Ok(())
    }

    pub fn currently_sharing_blobs(&self) -> &HashSet<iroh_blobs::Hash> {
        &self.state.currently_sharing_blobs
    }

    pub async fn node_addr(&self) -> Result<NodeAddr> {
        self.router.endpoint().node_addr().await
    }

    pub async fn join_ticket(&self) -> Result<String> {
        let me = self.router.endpoint().node_addr().await?;
        Ok(PeerList(vec![me]).to_string())
    }

    /// RemoteInfo and bandwidth in bytes/s for a node
    pub fn remote_infos(&self) -> Vec<(RemoteInfo, f64)> {
        self.router
            .endpoint()
            .remote_info_iter()
            .map(|node_info| {
                let bandwidth = self
                    .state
                    .bandwidth_tracker
                    .get_bandwidth_by_node(&node_info.node_id)
                    .unwrap_or_default();
                (node_info, bandwidth)
            })
            .collect()
    }

    pub async fn poll_next(&mut self) -> Result<Option<NetworkEvent<BroadcastMessage, Download>>> {
        // these are factored out to separate fns so rustfmt works on their contents :)
        select! {
            Some(event) = self.gossip_rx.next() => {
                if let Some(result) = parse_gossip_event(event) {
                    return Ok(Some(NetworkEvent::MessageReceived(result)));
                }
            }
            update = self.download_manager.poll_next() => {
                match update {
                    Some(DownloadManagerEvent::Complete(result)) => {
                        return Ok(Some(NetworkEvent::DownloadComplete(result)))
                    }
                    Some(DownloadManagerEvent::Update(update)) => {
                        self.on_download_update(update)?;
                    },
                    Some(DownloadManagerEvent::Failed(result)) => {
                        return Ok(Some(NetworkEvent::DownloadFailed(result)))
                    }
                    None => {}
                }
            }
            Some(ParameterSharingMessage::Get(parameter_name, protocol_req_tx)) = self.rx_model_parameter_req.recv() => {
                return Ok(Some(NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx)));
            }
            _ = self.update_stats_interval.tick() => {
                on_update_stats(self.router.endpoint(), &mut self.state).await?;
            }
        };

        Ok(None)
    }

    fn on_download_update(&mut self, update: DownloadUpdate) -> Result<()> {
        self.state.bandwidth_tracker.add_event(
            update.blob_ticket.node_addr().node_id,
            update.downloaded_size_delta,
        );

        let hash = update.blob_ticket.hash();

        if update.all_done {
            self.state.download_progesses.remove(&hash);

            let download = match update.error {
                Some(err) => Err(Error::msg(err.to_string())),
                None => {
                    let blobs = self.blobs.client().clone();
                    let (send, recv) = oneshot::channel();
                    tokio::spawn(async move {
                        let blob_bytes = match blobs.read_to_bytes(hash).await {
                            Ok(b) => b,
                            Err(e) => {
                                error!("Failed to read bytes: {e}");
                                return;
                            }
                        };
                        let res = send.send(blob_bytes);
                        if res.is_err() {
                            error!("Failed to send read bytes result.");
                        }
                    });
                    Ok(recv)
                }
            };
            self.download_manager.read(update.blob_ticket, download);
        } else {
            self.state.download_progesses.insert(hash, update);
        }
        Ok(())
    }

    pub async fn get_all_peers(&self) -> PeerList {
        PeerList(
            std::iter::once(
                self.router
                    .endpoint()
                    .node_addr()
                    .await
                    .expect("node addr exists"),
            )
            .chain(
                self.router
                    .endpoint()
                    .remote_info_iter()
                    .map(NodeAddr::from),
            )
            .collect(),
        )
    }
}

fn parse_gossip_event<BroadcastMessage: Networkable>(
    event: Result<iroh_gossip::net::Event>,
) -> Option<(PublicKey, BroadcastMessage)> {
    if let Ok(iroh_gossip::net::Event::Gossip(GossipEvent::Received(msg))) = event {
        if let Ok(result) = SignedMessage::<BroadcastMessage>::verify_and_decode(&msg.content) {
            return Some(result);
        }
    }

    None
}

#[derive(Debug)]
pub enum NetworkEvent<BM, D>
where
    BM: Networkable,
    D: Networkable,
{
    MessageReceived((PublicKey, BM)),
    DownloadComplete(DownloadComplete<D>),
    DownloadFailed(DownloadFailed),
    ParameterRequest(String, oneshot::Sender<String>),
}

async fn on_update_stats(endpoint: &Endpoint, stats: &mut State) -> Result<()> {
    let ticket = {
        let me = endpoint.node_addr().await?;
        PeerList(vec![me])
    };

    stats.join_ticket = ticket;

    for (peer_id, conn_type, last_recvd) in endpoint
        .remote_info_iter()
        .filter_map(|i| i.last_received().map(|r| (i.node_id, i.conn_type, r)))
    {
        // after 2 minutes with no comms, assume a client is disconnected.
        if last_recvd.as_secs() < 120 {
            stats
                .last_seen
                .insert(peer_id, (conn_type, Instant::now().sub(last_recvd)));
        } else {
            stats.last_seen.remove(&peer_id);
        }
    }

    stats
        .bandwidth_history
        .push_back(stats.bandwidth_tracker.get_total_bandwidth());
    const BANDWIDTH_GRAPH_SIZE: usize = 60;
    if stats.bandwidth_history.len() > BANDWIDTH_GRAPH_SIZE {
        stats.bandwidth_history.pop_front();
    }

    Ok(())
}
