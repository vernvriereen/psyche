use crate::protocol::{ClientId, NC};
use crate::{protocol::Message, tui::TUIState};

use anyhow::Result;
use psyche_client::payload::Payload;
use psyche_coordinator::coordinator::Coordinator;
use psyche_network::NetworkEvent;
use rand::RngCore;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{select, time::Interval};

struct Backend {
    network: NC,
    pending_clients: Vec<ClientId>,
}

impl psyche_coordinator::traits::Backend<ClientId> for Backend {
    fn select_new_clients(&self) -> &[ClientId] {
        &self.pending_clients
    }
}

pub struct App {
    tx_tui_state: Sender<TUIState>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
}

impl App {
    pub fn new(
        network: NC,
        tx_tui_state: Sender<TUIState>,
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
        let tui_state = TUIState {
            coordinator: (&self.coordinator).into(),
            network: (&self.backend.network).into(),
        };
        self.tx_tui_state.send(tui_state)?;
        Ok(())
    }

    fn on_network_event(&mut self, _event: NetworkEvent<Message, Payload>) {}

    async fn on_tick(&mut self) {
        let unix_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.coordinator
            .tick(&self.backend, unix_timestamp, rand::thread_rng().next_u64());
    }
}
