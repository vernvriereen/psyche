use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_coordinator::model::{
    self, Checkpoint, LLMTrainingDataLocation, LLMTrainingDataType, Model, LLM,
};
use psyche_coordinator::{
    Client, ClientState, Coordinator, CoordinatorError, HealthChecks, Round, RunState, TickResult,
    Witness, SOLANA_MAX_NUM_CLIENTS,
};

use psyche_core::{u8_to_string, FixedVec, Shuffle, SizedIterator, TokenSize};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpServer, DataServerTui, LocalDataProvider,
};
use psyche_network::{ClientNotification, TcpServer};
use psyche_tui::{
    logging::LoggerWidget, maybe_start_render_loop, CustomWidget, MaybeTui, TabbedWidget,
};
use psyche_watcher::CoordinatorTui;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::{interval, MissedTickBehavior};
use tokio::{select, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, warn, Instrument};

use crate::dashboard::{DashboardState, DashboardTui};

pub(super) type Tabs = TabbedWidget<(
    DashboardTui,
    CoordinatorTui,
    MaybeTui<DataServerTui>,
    LoggerWidget,
)>;
pub(super) const TAB_NAMES: [&str; 4] =
    ["Dashboard", "Coordinator", "Training Data Server", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

struct Backend {
    net_server: TcpServer<ClientId, ClientToServerMessage, ServerToClientMessage>,
    pending_clients: HashSet<ClientId>,
}

impl Backend {
    pub fn port(&self) -> u16 {
        self.net_server.local_addr().port()
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

    async fn send_witness(&mut self, _witness: Witness) -> Result<()> {
        bail!("Server does not send witnesses");
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> Result<()> {
        bail!("Server does not send health checks");
    }

    async fn send_checkpoint(&mut self, _checkpoint: model::Checkpoint) -> Result<()> {
        bail!("Server does not send checkpoints");
    }
}

type DataServer =
    DataProviderTcpServer<ClientId, ClientId, LocalDataProvider, ChannelCoordinatorBackend>;

pub struct App {
    cancel: CancellationToken,
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
    training_data_server: Option<(Sender<Coordinator<ClientId>>, DataServer)>,
    save_state_dir: Option<PathBuf>,
    original_warmup_time: u64,
    original_min_clients: u16,
    withdraw_on_disconnect: bool,
}

/// Methods intended for testing purposes only.
///
/// These methods provide access to internal App parameters
/// to facilitate testing and debugging.
#[allow(dead_code)]
impl App {
    pub fn get_clients(&self) -> FixedVec<Client<ClientId>, SOLANA_MAX_NUM_CLIENTS> {
        self.coordinator.epoch_state.clients
    }

    pub fn get_pending_clients(&self) -> HashSet<ClientId> {
        self.backend.pending_clients.clone()
    }

    pub fn get_run_state(&self) -> RunState {
        self.coordinator.run_state
    }

    pub fn get_rounds(&self) -> [Round; 4] {
        self.coordinator.epoch_state.rounds
    }

    pub fn get_rounds_head(&self) -> u32 {
        self.coordinator.epoch_state.rounds_head
    }

    pub fn get_current_epoch(&self) -> u16 {
        self.coordinator.progress.epoch
    }

    pub fn get_port(&self) -> u16 {
        self.backend.port()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataServerInfo {
    pub dir: PathBuf,
    pub token_size: TokenSize,
    pub seq_len: usize,
    pub shuffle_seed: [u8; 32],
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        tui: bool,
        mut coordinator: Coordinator<ClientId>,
        data_server_config: Option<DataServerInfo>,
        coordinator_server_port: Option<u16>,
        save_state_dir: Option<PathBuf>,
        init_warmup_time: Option<u64>,
        init_min_clients: Option<u16>,
        withdraw_on_disconnect: bool,
    ) -> Result<Self> {
        async {
            Self::reset_ephemeral(&mut coordinator);

            debug!("potentially launching data server...");

            let training_data_server = if let Model::LLM(LLM {
                data_location: LLMTrainingDataLocation::Server(url),
                data_type,
                checkpoint,
                ..
            }) = &coordinator.model
            {
                if let LLMTrainingDataType::Finetuning = data_type {
                    panic!("Finetuning is not supported yet.")
                }

                match checkpoint {
                    Checkpoint::Hub(hub_repo) => {
                        let repo_id = u8_to_string(&hub_repo.repo_id);
                        let revision = hub_repo.revision.map(|bytes| u8_to_string(&bytes));
                        if revision.is_some()
                            || !tokio::fs::try_exists(PathBuf::from(repo_id.clone()))
                                .await
                                .unwrap_or_default()
                        {
                            download_model_repo_async(&repo_id, revision, None, None, None, true)
                                .await?;
                        }
                    }
                    Checkpoint::Ephemeral => {
                        bail!("Can't start up a run with an Ephemeral checkpoint.")
                    }
                    Checkpoint::Dummy => {
                        // ok!
                    }
                    Checkpoint::P2P => {
                        bail!("Can't start up a run with a P2P checkpoint.")
                    }
                }

                let server_addr: SocketAddr = u8_to_string(url).parse().map_err(|e| {
                    anyhow!("Failed to parse training data server URL {:?}: {}", url, e)
                })?;
                let data_server_port = server_addr.port();
                let DataServerInfo {
                    dir,
                    seq_len,
                    shuffle_seed,
                    token_size
                } = data_server_config.ok_or_else(|| anyhow!("Coordinator state requires we host training data, but no --data-config passed."))?;

                let local_data_provider = LocalDataProvider::new_from_directory(
                    dir,
                    token_size,
                    seq_len,
                    Shuffle::Seeded(shuffle_seed),
                )?;

                let (tx, backend) = ChannelCoordinatorBackend::new();
                let data_server =
                    DataProviderTcpServer::start(local_data_provider, backend, data_server_port)
                        .await?;
                Some((tx, data_server))
            } else {
                None
            };
            debug!("data server work done.");

            let (cancel, tx_tui_state) =
                maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

            let mut tick_interval = interval(Duration::from_millis(500));
            tick_interval.set_missed_tick_behavior(MissedTickBehavior::Skip); //important!

            let mut update_tui_interval = interval(Duration::from_millis(150));
            update_tui_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let net_server =
                TcpServer::<ClientId, ClientToServerMessage, ServerToClientMessage>::start(
                    SocketAddr::new(
                        std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                        coordinator_server_port.unwrap_or(0),
                    ),
                )
                .await?;

            let original_warmup_time = coordinator.config.warmup_time;
            let original_min_clients = coordinator.config.min_clients;

            if let Some(init_warmup_time) = init_warmup_time {
                coordinator.config.warmup_time = init_warmup_time;
            }
            if let Some(init_min_clients) = init_min_clients {
                coordinator.config.min_clients = init_min_clients;
            }

            Ok(Self {
                cancel,
                training_data_server,
                tx_tui_state,
                tick_interval,
                update_tui_interval,
                coordinator,
                backend: Backend {
                    net_server,
                    pending_clients: HashSet::new(),
                },
                save_state_dir,
                original_warmup_time,
                original_min_clients,
                withdraw_on_disconnect,
            })
        }.instrument(info_span!("App::new")).await
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            if let ControlFlow::Break(()) = self.poll_next().await? {
                break;
            }
        }
        Ok(())
    }

    pub async fn poll_next(&mut self) -> Result<ControlFlow<(), ()>> {
        select! {
            _ = self.cancel.cancelled() => {
                info!("got cancel callback, exiting cleanly.");
                return Ok(ControlFlow::Break(()));
            }

            Some(event) = self.backend.net_server.next() => {
                match event {
                    ClientNotification::Message((from, message)) => {
                        self.on_client_message(from, message).await;
                    }
                    ClientNotification::Disconnected(from) => {
                        self.on_disconnect(from)?;
                    }
                }
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
                } else {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
        Ok(ControlFlow::Continue(()))
    }

    async fn update_tui(&mut self) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                (&*self).into(),
                (&self.coordinator).into(),
                self.training_data_server.as_ref().map(|o| (&o.1).into()),
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }

    fn on_disconnect(&mut self, from: ClientId) -> Result<()> {
        self.backend.pending_clients.remove(&from);

        if self.withdraw_on_disconnect {
            let position = self
                .coordinator
                .epoch_state
                .clients
                .iter()
                .position(|x| x.id == from);

            if let Some(index) = position {
                match self.coordinator.withdraw(index as u64) {
                    Ok(_) => info!("Withdrew {from}"),
                    Err(err) => warn!("Coordinator withdraw error: {err}"),
                }
            }
        }

        Ok(())
    }

    async fn on_client_message(&mut self, from: ClientId, event: ClientToServerMessage) {
        let broadcast = match event {
            ClientToServerMessage::Join { run_id } => {
                // TODO: check whitelist
                let coord_run_id = u8_to_string(&self.coordinator.run_id);
                if coord_run_id == run_id {
                    info!("added pending client {from}");
                    self.backend.pending_clients.insert(from);
                } else {
                    info!("{from:?} tried to join unknown run {run_id}");
                }
                false
            }
            ClientToServerMessage::Witness(witness) => {
                let state_before = self.coordinator.run_state;
                if let Err(error) = self
                    .coordinator
                    .witness(&from, *witness, Self::get_timestamp())
                {
                    warn!("Error when processing witness: {error}");
                }
                self.coordinator.run_state != state_before
            }
            ClientToServerMessage::HealthCheck(health_checks) => {
                match self.coordinator.health_check(&from, health_checks) {
                    Ok(dropped) => {
                        info!("Dropped {} clients from health check", dropped);
                        dropped > 0
                    }

                    Err(error) => {
                        warn!("Error when processing health check: {error}");
                        false
                    }
                }
            }
            ClientToServerMessage::Checkpoint(checkpoint) => {
                if let Err(error) = self.coordinator.checkpoint(&from, checkpoint) {
                    warn!("Error when processing checkpoint: {error}");
                }
                true
            }
        };
        self.post_state_change(broadcast).await;
    }

    async fn on_tick(&mut self) {
        self.remove_unhealthy_clients();
        match self.coordinator.tick(
            Some(SizedIterator::new(
                self.backend.pending_clients.iter(),
                self.backend.pending_clients.len(),
            )),
            Self::get_timestamp(),
            rand::thread_rng().next_u64(),
        ) {
            Ok(TickResult::EpochEnd(result)) => {
                if result {
                    if let Some(save_state_dir) = &self.save_state_dir {
                        let mut state = self.coordinator;
                        print!("{:?}", state);
                        Self::reset_ephemeral(&mut state);
                        match toml::to_string_pretty(&state) {
                            Ok(toml) => {
                                let filename = format!(
                                    "{:?}-step{}.toml",
                                    self.coordinator.run_id,
                                    self.coordinator.progress.step - 1
                                );
                                info!("Saving state to {filename}");
                                if let Err(err) =
                                    std::fs::write(save_state_dir.join(filename), toml)
                                {
                                    tracing::error!("Error saving TOML: {}", err);
                                }
                            }
                            Err(err) => tracing::error!("Error serialized to TOML: {err}"),
                        }
                    }
                } else {
                    warn!("Epoch abandoned")
                }
            }
            Ok(TickResult::Ticked) | Err(CoordinatorError::Halted) => {}
            Err(err) => warn!("Coordinator tick error: {err}"),
        }
        self.post_state_change(true).await;
    }

    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    async fn post_state_change(&mut self, broadcast: bool) {
        if self.coordinator.active() {
            // reset to original values if we changed them to something special for init
            self.coordinator.config.warmup_time = self.original_warmup_time;
            self.coordinator.config.min_clients = self.original_min_clients;
        }
        if broadcast {
            if let Err(err) = self
                .backend
                .net_server
                .broadcast(ServerToClientMessage::Coordinator(Box::new(
                    self.coordinator,
                )))
                .await
            {
                warn!("Error in on_tick: {err}");
            }
            if let Some((ref sender, _)) = self.training_data_server {
                sender.send(self.coordinator).await.unwrap();
            }
        }
    }

    fn reset_ephemeral(coordinator: &mut Coordinator<ClientId>) {
        coordinator.run_state = RunState::WaitingForMembers;
        for elem in coordinator.epoch_state.clients.iter_mut() {
            *elem = Client::<ClientId>::default();
        }
        for elem in coordinator.epoch_state.exited_clients.iter_mut() {
            *elem = Client::<ClientId>::default();
        }
    }

    fn remove_unhealthy_clients(&mut self) {
        for client in self.coordinator.epoch_state.exited_clients {
            if client.state != ClientState::Healthy {
                self.backend.pending_clients.remove(&client.id);
            }
        }
    }
}

impl From<&App> for DashboardState {
    fn from(app: &App) -> Self {
        Self {
            coordinator_state: (&app.coordinator).into(),
            server_addr: app.backend.net_server.local_addr().to_string(),
            nodes_next_epoch: app
                .backend
                .pending_clients
                .iter()
                .map(|c| c.to_string())
                .collect(),
        }
    }
}
