use anyhow::Result;
use psyche_centralized_shared::{
    BroadcastMessage, ClientId, ClientToServerMessage, Payload, ServerToClientMessage, NC,
};
use psyche_coordinator::Coordinator;
use psyche_network::{NetworkEvent, NetworkTUI, TcpClient};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::CoordinatorTui;
use std::mem::replace;
use std::sync::mpsc::Sender;
use tokio::{select, time::Interval};
use tracing::info;

pub(super) type Tabs = TabbedWidget<(CoordinatorTui, NetworkTUI, LoggerWidget)>;
pub(super) const TAB_NAMES: [&str; 3] = ["Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

pub struct App {
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator_state: Coordinator<ClientId>,
    last_coordinator_state: Coordinator<ClientId>,
    p2p: NC,
    server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,
}

impl App {
    pub fn new(
        p2p: NC,
        server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,
        tx_tui_state: Option<Sender<TabsData>>,
        tick_interval: Interval,
        update_tui_interval: Interval,
    ) -> Self {
        Self {
            tx_tui_state,
            tick_interval,
            update_tui_interval,
            last_coordinator_state: Coordinator::default(),
            coordinator_state: Coordinator::default(),
            p2p,
            server_conn,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.server_conn.send(ClientToServerMessage::Join).await?;
        loop {
            select! {
                Ok(Some(event)) = self.p2p.poll_next() => {
                    self.on_peer_network_event(event).await;
                }
                Ok(message) = self.server_conn.receive() => {
                    self.on_server_message(message).await;
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
                (&self.coordinator_state).into(),
                (&self.p2p).into(),
                Default::default(),
            );
            tx_tui_state.send(states)?;
        }
        Ok(())
    }

    async fn on_peer_network_event(&mut self, event: NetworkEvent<BroadcastMessage, Payload>) {
        if let NetworkEvent::MessageReceived((from, message)) = event {
            info!(
                "got network event broadcasted from {:?}! {:?}",
                from, message
            );
        }
    }

    async fn on_server_message(&mut self, message: ServerToClientMessage) {
        match message {
            ServerToClientMessage::P2PConnect(_) => {
                // ignore.
            }
            ServerToClientMessage::Coordinator(state) => {
                let prev_state = replace(&mut self.coordinator_state, state);
                self.last_coordinator_state = prev_state;
                // TODO on state change!
            }
        }
    }

    async fn on_tick(&mut self) {
        // read coordinator state maybe? maybe no need.
    }
}
