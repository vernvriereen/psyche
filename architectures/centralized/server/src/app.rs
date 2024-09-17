use anyhow::Result;
use psyche_centralized_shared::{
    BroadcastMessage, ClientId, ClientToServerMessage, Payload, ServerToClientMessage, NC,
};
use psyche_coordinator::Coordinator;
use psyche_network::{NetworkEvent, NetworkTUI, PeerList, TcpServer};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::CoordinatorTui;
use rand::RngCore;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{select, time::Interval};
use tracing::warn;

use crate::dashboard::{DashboardState, DashboardTui};

pub(super) type Tabs = TabbedWidget<(DashboardTui, CoordinatorTui, NetworkTUI, LoggerWidget)>;
pub(super) const TAB_NAMES: [&str; 4] = ["Dashboard", "Coordinator", "P2P Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

struct Backend {
    net_server: TcpServer<ClientId, ClientToServerMessage, ServerToClientMessage>,
    pending_clients: Vec<ClientId>,
}

impl psyche_coordinator::Backend<ClientId> for Backend {
    fn select_new_clients(&self) -> &[ClientId] {
        &self.pending_clients
    }
}

pub struct App {
    run_id: String,
    p2p: NC,
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
}

impl App {
    pub fn new(
        run_id: String,
        p2p: NC,
        net_server: TcpServer<ClientId, ClientToServerMessage, ServerToClientMessage>,
        tx_tui_state: Option<Sender<TabsData>>,
        tick_interval: Interval,
        update_tui_interval: Interval,
    ) -> Self {
        Self {
            run_id,
            p2p,
            tx_tui_state,
            tick_interval,
            update_tui_interval,
            coordinator: Coordinator::default(),
            backend: Backend {
                net_server,
                pending_clients: Vec::new(),
            },
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
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
                    self.update_tui()?;
                }
                else => break,
            }
        }
        Ok(())
    }

    fn update_tui(&mut self) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                (&*self).into(),
                (&self.coordinator).into(),
                (&self.p2p).into(),
                Default::default(),
            );
            tx_tui_state.send(states)?;
        }
        Ok(())
    }

    fn on_network_event(&mut self, event: NetworkEvent<BroadcastMessage, Payload>) {
        if let NetworkEvent::MessageReceived((from, message)) = event {
            {
                warn!(
                    "got gossip message we don't handle yet {:?} {:?}",
                    from, message
                );
            }
        }
    }

    async fn on_client_message(&mut self, from: ClientId, event: ClientToServerMessage) {
        match event {
            ClientToServerMessage::Join => {
                self.backend.pending_clients.push(from.clone());
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
    }
}

impl From<&App> for DashboardState {
    fn from(app: &App) -> Self {
        Self {
            run_id: app.run_id.clone(),
            coordinator_state: (&app.coordinator).into(),
            server_addr: app.backend.net_server.local_addr().to_string(),
        }
    }
}
