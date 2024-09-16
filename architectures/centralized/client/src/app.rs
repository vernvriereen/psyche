use anyhow::Result;
use psyche_centralized_shared::{ClientId, Message, Payload, NC};
use psyche_coordinator::Coordinator;
use psyche_network::{NetworkEvent, NetworkTUI};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::CoordinatorTUI;
use rand::RngCore;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{select, time::Interval};

pub(super) type Tabs = TabbedWidget<(CoordinatorTUI, NetworkTUI, LoggerWidget)>;
pub(super) const TAB_NAMES: [&str; 3] = ["Coordinator", "Network", "Logger"];
type TabsData = <TabbedWidget<(CoordinatorTUI, NetworkTUI, LoggerWidget)> as CustomWidget>::Data;

struct Backend {
    network: NC,
    pending_clients: Vec<ClientId>,
}

impl psyche_coordinator::Backend<ClientId> for Backend {
    fn select_new_clients(&self) -> &[ClientId] {
        &self.pending_clients
    }
}

pub struct App {
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
}

impl App {
    pub fn new(
        network: NC,
        tx_tui_state: Option<Sender<TabsData>>,
        tick_interval: Interval,
        update_tui_interval: Interval,
    ) -> Self {
        Self {
            tx_tui_state,
            tick_interval,
            update_tui_interval,
            coordinator: Coordinator::default(),
            backend: Backend {
                network,
                pending_clients: Vec::new(),
            },
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.backend.network.broadcast(&Message::Join).await?;
        loop {
            select! {
                Ok(Some(event)) = self.backend.network.poll_next() => {
                    self.on_network_event(event);
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
                (&self.coordinator).into(),
                (&self.backend.network).into(),
                Default::default(),
            );
            tx_tui_state.send(states)?;
        }
        Ok(())
    }

    fn on_network_event(&mut self, event: NetworkEvent<Message, Payload>) {
        if let NetworkEvent::MessageReceived((_, message)) = event { match message {
            Message::Join => {
                // ignore :)
            }
            Message::Coordinator(state) => {
                self.coordinator = state;
            }
        } }
    }

    async fn on_tick(&mut self) {
        let unix_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.coordinator
            .tick(&self.backend, unix_timestamp, rand::thread_rng().next_u64());
    }
}
