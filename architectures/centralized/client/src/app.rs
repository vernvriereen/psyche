use std::path::PathBuf;

use anyhow::{Error, Result};
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{Client, ClientTUI, ClientTUIState, NC};
use psyche_coordinator::{Coordinator, HealthChecks, Witness};
use psyche_network::{NetworkTUIState, NetworkTui, SecretKey, TcpClient};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::{Backend as WatcherBackend, CoordinatorTui};
use tokio::sync::mpsc::Sender;
use tokio::{select, sync::mpsc, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub(super) const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

enum ToSend {
    Witness(Witness),
    HealthCheck(HealthChecks),
}

struct Backend {
    rx: mpsc::Receiver<Coordinator<ClientId>>,
    tx: mpsc::Sender<ToSend>,
}

#[async_trait::async_trait]
impl WatcherBackend<ClientId> for Backend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<ClientId>> {
        self.rx
            .recv()
            .await
            .ok_or(Error::msg("watcher backend rx channel closed"))
    }

    async fn send_witness(&mut self, witness: Witness) -> Result<()> {
        self.tx.send(ToSend::Witness(witness)).await?;
        Ok(())
    }

    async fn send_health_check(&mut self, health_checks: HealthChecks) -> Result<()> {
        self.tx.send(ToSend::HealthCheck(health_checks)).await?;
        Ok(())
    }
}

pub struct App {
    cancel: CancellationToken,
    secret_key: SecretKey,
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator_state: Coordinator<ClientId>,
    server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,
    run_id: String,
    data_parallelism: usize,
    tensor_parallelism: usize,
    write_gradients_dir: Option<PathBuf>,
}

impl App {
    pub fn new(
        cancel: CancellationToken,
        secret_key: SecretKey,
        server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,
        tx_tui_state: Option<Sender<TabsData>>,
        tick_interval: Interval,
        update_tui_interval: Interval,
        run_id: &str,
        data_parallelism: usize,
        tensor_parallelism: usize,
        write_gradients_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            cancel,
            secret_key,
            tx_tui_state,
            tick_interval,
            update_tui_interval,
            coordinator_state: Coordinator::default(),
            server_conn,
            run_id: run_id.into(),
            data_parallelism,
            tensor_parallelism,
            write_gradients_dir,
        }
    }

    pub async fn run(&mut self, mut p2p: NC) -> Result<()> {
        self.server_conn
            .send(ClientToServerMessage::Join {
                run_id: self.run_id.clone(),
            })
            .await?;
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    return Ok(());
                }
                Ok(ServerToClientMessage::P2PConnect(peers)) = self.server_conn.receive() => {
                    p2p
                    .add_peers(peers.0)
                    .await?;
                    break;
                }
                _ = self.update_tui_interval.tick() => {
                    self.update_tui(Default::default(), Default::default()).await?;
                }
            }
        }
        let (tx, rx) = mpsc::channel(10);
        let (witness_tx, mut witness_rx) = mpsc::channel(10);
        let identity = ClientId::from(p2p.node_addr().await?.node_id);
        let mut client = Client::new(
            Backend { rx, tx: witness_tx },
            p2p,
            identity,
            self.secret_key.clone(),
            self.data_parallelism,
            self.tensor_parallelism,
            self.write_gradients_dir.clone(),
        );

        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                message = self.server_conn.receive() => {
                    self.on_server_message(message?, &tx).await;
                }
                _ = self.tick_interval.tick() => {
                    self.on_tick().await;
                }
                _ = self.update_tui_interval.tick() => {
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    self.update_tui(client_tui_state, network_tui_state).await?;
                }
                res = client.process() => {
                    res?;
                }
                Some(to_send) = witness_rx.recv() => {
                    match to_send {
                        ToSend::Witness(witness) => self.server_conn.send(ClientToServerMessage::Witness(witness)).await?,
                        ToSend::HealthCheck(health_checks) => self.server_conn.send(ClientToServerMessage::HealthCheck(health_checks)).await?,
                    };
                }
            }
        }
        Ok(())
    }

    async fn update_tui(
        &mut self,
        client_tui_state: ClientTUIState,
        network_tui_state: NetworkTUIState,
    ) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                client_tui_state,
                (&self.coordinator_state).into(),
                network_tui_state,
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }

    async fn on_server_message(
        &mut self,
        message: ServerToClientMessage,
        tx: &mpsc::Sender<Coordinator<ClientId>>,
    ) {
        match message {
            ServerToClientMessage::P2PConnect(_peers) => {
                info!("Got peer list from server, but already connected");
            }
            ServerToClientMessage::Coordinator(state) => {
                self.coordinator_state = state.clone();
                let _ = tx.send(state).await;
            }
        }
    }

    async fn on_tick(&mut self) {
        // read coordinator state maybe? maybe no need.
    }
}
