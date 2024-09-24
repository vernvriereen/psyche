use anyhow::{anyhow, Result};
use async_trait::async_trait;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{BroadcastMessage, Payload, NC};
use psyche_coordinator::model::{LLMTrainingDataLocation, LLMTrainingDataType, Model, LLM};
use psyche_coordinator::{Client, Coordinator};
use psyche_data_provider::{DataProviderTcpServer, DataServerTui, LocalDataProvider, TokenSize};
use psyche_network::{NetworkEvent, NetworkTui, PeerList, RelayMode, TcpServer};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{maybe_start_render_loop, CustomWidget, MaybeTui, TabbedWidget};
use psyche_watcher::CoordinatorTui;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::interval;
use tokio::{select, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::dashboard::{DashboardState, DashboardTui};

pub(super) type Tabs = TabbedWidget<(
    DashboardTui,
    CoordinatorTui,
    NetworkTui,
    MaybeTui<DataServerTui>,
    LoggerWidget,
)>;
pub(super) const TAB_NAMES: [&str; 5] = [
    "Dashboard",
    "Coordinator",
    "P2P Network",
    "Training Data Server",
    "Logger",
];
type TabsData = <Tabs as CustomWidget>::Data;

struct Backend {
    net_server: TcpServer<ClientId, ClientToServerMessage, ServerToClientMessage>,
    pending_clients: Vec<Client<ClientId>>,
}

impl psyche_coordinator::Backend<ClientId> for Backend {
    fn select_new_clients(&self) -> &[Client<ClientId>] {
        &self.pending_clients
    }
}

struct ChannelCoordinatorBackend {
    rx: Receiver<Coordinator<ClientId>>,
}

impl ChannelCoordinatorBackend {
    fn new() -> (Sender<Coordinator<ClientId>>, Self) {
        let (tx, rx) = channel(10);
        (tx, Self { rx })
    }
}

#[async_trait]
impl psyche_watcher::Backend<ClientId> for ChannelCoordinatorBackend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<ClientId>> {
        Ok(self.rx.recv().await.expect("channel closed? :("))
    }
}

pub struct App {
    cancel: CancellationToken,
    p2p: NC,
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
    training_data_server: Option<(
        Sender<Coordinator<ClientId>>,
        DataProviderTcpServer<ClientId, LocalDataProvider, ChannelCoordinatorBackend>,
    )>,
}

#[derive(Serialize, Deserialize)]
pub struct DataServerInfo {
    pub dir: PathBuf,
    pub token_size: TokenSize,
    pub seq_len: usize,
    pub shuffle_seed: [u8; 32],
}

impl App {
    pub async fn new(
        tui: bool,
        coordinator: Coordinator<ClientId>,
        data_server_config: Option<DataServerInfo>,
        p2p_port: Option<u16>,
        server_port: Option<u16>,
    ) -> Result<Self> {
        let p2p = NC::init(
            &coordinator.run_id,
            p2p_port,
            RelayMode::Default,
            vec![],
            None,
        )
        .await?;

        let training_data_server = if let Some(Model::LLM(LLM {
            data_location: LLMTrainingDataLocation::Server(url),
            data_type,
            ..
        })) = &coordinator.model
        {
            if let LLMTrainingDataType::Finetuning = data_type {
                panic!("Finetuning is not supported yet.")
            }
            let server_addr: SocketAddr = url
                .parse()
                .map_err(|e| anyhow!("Failed to parse training data server URL {url}: {e}"))?;
            let server_port = server_addr.port();
            let DataServerInfo {
                dir,
               seq_len,
               shuffle_seed,
               token_size
            } = data_server_config.ok_or_else(|| anyhow!("Coordinator state requires we host training data, but no --data-config passed."))?;
            let local_data_provider =
                LocalDataProvider::new_from_directory(dir, token_size, seq_len, shuffle_seed)?;
            let (tx, backend) = ChannelCoordinatorBackend::new();
            let data_server =
                DataProviderTcpServer::start(local_data_provider, backend, server_port).await?;
            Some((tx, data_server))
        } else {
            None
        };

        let (cancel, tx_tui_state) =
            maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

        let tick_interval = interval(Duration::from_secs(1));

        let update_tui_interval = interval(Duration::from_millis(150));

        let net_server =
            TcpServer::<ClientId, ClientToServerMessage, ServerToClientMessage>::start(
                SocketAddr::new(
                    std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                    server_port.unwrap_or(0),
                ),
            )
            .await?;

        Ok(Self {
            cancel,
            training_data_server,
            p2p,
            tx_tui_state,
            tick_interval,
            update_tui_interval,
            coordinator,
            backend: Backend {
                net_server,
                pending_clients: Vec::new(),
            },
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    return Ok(());
                }

                Ok(Some(event)) = self.p2p.poll_next() => {
                    self.on_network_event(event);
                }
                Some(event) = self.backend.net_server.next() => {
                    self.on_client_message(event.0, event.1).await;
                }
                _ = self.tick_interval.tick() => {
                    self.on_tick().await;
                }
                _ = self.update_tui_interval.tick() => {
                    self.update_tui().await?;
                }
                _ = async {
                    if let Some((_, server))  = &mut self.training_data_server {
                        server.poll().await
                    }
                } => {}
                else => break,
            }
        }
        Ok(())
    }

    async fn update_tui(&mut self) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                (&*self).into(),
                (&self.coordinator).into(),
                (&self.p2p).into(),
                self.training_data_server.as_ref().map(|o| (&o.1).into()),
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }

    fn on_network_event(&mut self, event: NetworkEvent<BroadcastMessage, Payload>) {
        if let NetworkEvent::MessageReceived((from, message)) = event {
            warn!(
                "got gossip message we don't handle yet {:?} {:?}",
                from, message
            );
        }
    }

    async fn on_client_message(&mut self, from: ClientId, event: ClientToServerMessage) {
        match event {
            ClientToServerMessage::Join { run_id, data_bid } => {
                // TODO: check whitelist
                if self.coordinator.run_id == run_id {
                    self.backend.pending_clients.push(Client {
                        id: from.clone(),
                        num_data_indicies: data_bid,
                    });
                    let client_joined = self
                        .backend
                        .net_server
                        .send_to(
                            from,
                            ServerToClientMessage::P2PConnect(PeerList(vec![self
                                .p2p
                                .node_addr()
                                .await
                                .expect("node addr works..")])),
                        )
                        .await;
                    if let Err(e) = client_joined {
                        warn!("Error sending p2p list to client: {e}");
                    }
                } else {
                    info!("{from:?} tried to join unknown run {run_id}");
                }
            }
        }
    }

    async fn on_tick(&mut self) {
        let unix_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.coordinator
            .tick(&self.backend, unix_timestamp, rand::thread_rng().next_u64());
        if let Err(err) = self
            .backend
            .net_server
            .broadcast(ServerToClientMessage::Coordinator(self.coordinator.clone()))
            .await
        {
            warn!("error in tick: {err}");
        }
        if let Some((ref sender, _)) = self.training_data_server {
            sender.send(self.coordinator.clone()).await.unwrap();
        }
    }
}

impl From<&App> for DashboardState {
    fn from(app: &App) -> Self {
        Self {
            coordinator_state: (&app.coordinator).into(),
            server_addr: app.backend.net_server.local_addr().to_string(),
        }
    }
}
