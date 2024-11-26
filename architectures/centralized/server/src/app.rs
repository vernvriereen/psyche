use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{BroadcastMessage, Payload, NC};
use psyche_coordinator::model::{
    self, Checkpoint, LLMTrainingDataLocation, LLMTrainingDataType, Model, LLM,
};
use psyche_coordinator::{Client, Coordinator, CoordinatorError, HealthChecks, RunState, Witness};
use psyche_data_provider::{
    download_model_repo_async, DataProviderTcpServer, DataServerTui, LocalDataProvider, Shuffle,
    TokenSize,
};
use psyche_network::{ClientNotification, NetworkEvent, NetworkTui, RelayMode, TcpServer};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{maybe_start_render_loop, CustomWidget, MaybeTui, TabbedWidget};
use psyche_watcher::CoordinatorTui;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
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
    pending_clients: HashSet<Client<ClientId>>,
}

impl psyche_coordinator::Backend<ClientId> for Backend {
    fn select_new_clients(&self) -> Vec<Client<ClientId>> {
        self.pending_clients.iter().cloned().collect()
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

type DataServer = DataProviderTcpServer<ClientId, LocalDataProvider, ChannelCoordinatorBackend>;

pub struct App {
    cancel: CancellationToken,
    p2p: NC,
    tx_tui_state: Option<Sender<TabsData>>,
    tick_interval: Interval,
    update_tui_interval: Interval,
    coordinator: Coordinator<ClientId>,
    backend: Backend,
    training_data_server: Option<(Sender<Coordinator<ClientId>>, DataServer)>,
    save_state_dir: Option<PathBuf>,
    last_sync_step: Option<u32>,
    original_warmup_time: u64,
    original_min_clients: u32,
}

impl App {
    pub fn get_clients(&self) -> usize {
        self.coordinator.clients.len()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataServerInfo {
    pub dir: PathBuf,
    pub token_size: TokenSize,
    pub seq_len: usize,
    pub shuffle_seed: [u8; 32],
}

impl DataServerInfo {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let mut data_config: DataServerInfo = toml::from_str(std::str::from_utf8(&std::fs::read(
            &config_path,
        )?)?)
        .with_context(|| format!("failed to parse data server config toml file {config_path:?}"))?;

        // data dir, if relative, should be relative to the config's path.
        if !data_config.dir.is_absolute() {
            let config_dir = Path::new(&config_path).parent().unwrap_or(Path::new(""));
            data_config.dir = config_dir.join(data_config.dir);
        }
        Ok(data_config)
    }
}

impl App {
    pub async fn new(
        tui: bool,
        mut coordinator: Coordinator<ClientId>,
        data_server_config: Option<DataServerInfo>,
        p2p_port: Option<u16>,
        server_port: Option<u16>,
        save_state_dir: Option<PathBuf>,
        init_warmup_time: Option<u64>,
        init_min_clients: Option<u32>,
    ) -> Result<Self> {
        dbg!(&coordinator);
        dbg!(&p2p_port);
        let p2p = NC::init(
            &coordinator.run_id,
            p2p_port,
            RelayMode::Default,
            vec![],
            None,
        )
        .await?;

        Self::reset_ephemeral(&mut coordinator);

        let training_data_server = if let Some(Model::LLM(LLM {
            data_location: LLMTrainingDataLocation::Server(url),
            data_type,
            checkpoint,
            ..
        })) = &coordinator.model
        {
            dbg!(&url);
            dbg!(&data_type);
            dbg!(&checkpoint);
            if let LLMTrainingDataType::Finetuning = data_type {
                panic!("Finetuning is not supported yet.")
            }

            if let Checkpoint::Hub(hub_repo) = checkpoint {
                dbg!(&hub_repo);
                if hub_repo.revision.is_some()
                    || !tokio::fs::try_exists(PathBuf::from(hub_repo.repo_id.clone()))
                        .await
                        .unwrap_or_default()
                {
                    download_model_repo_async(
                        hub_repo.repo_id.clone(),
                        hub_repo.revision.clone(),
                        None,
                        None,
                        None,
                        true,
                    )
                    .await?;
                }
            } else {
                bail!("Cannot start without a Hub checkpoint");
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
            dbg!("data");
            dbg!(&dir);
            dbg!(&seq_len);
            dbg!(&shuffle_seed);
            dbg!(&token_size);
            let local_data_provider = LocalDataProvider::new_from_directory(
                dir,
                token_size,
                seq_len,
                Shuffle::Seeded(shuffle_seed),
            )?;
            let (tx, backend) = ChannelCoordinatorBackend::new();
            let data_server =
                DataProviderTcpServer::start(local_data_provider, backend, server_port).await?;
            Some((tx, data_server))
        } else {
            None
        };

        let (cancel, tx_tui_state) =
            maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

        let tick_interval = interval(Duration::from_millis(500));

        let update_tui_interval = interval(Duration::from_millis(150));

        let net_server =
            TcpServer::<ClientId, ClientToServerMessage, ServerToClientMessage>::start(
                SocketAddr::new(
                    std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                    server_port.unwrap_or(0),
                ),
            )
            .await?;

        let original_warmup_time = coordinator.warmup_time;
        let original_min_clients = coordinator.min_clients;

        if let Some(init_warmup_time) = init_warmup_time {
            coordinator.warmup_time = init_warmup_time;
        }
        if let Some(init_min_clients) = init_min_clients {
            coordinator.min_clients = init_min_clients;
        }

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
                pending_clients: HashSet::new(),
            },
            save_state_dir,
            last_sync_step: None,
            original_warmup_time,
            original_min_clients,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("IMPRIMIIIII");
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    info!("got cancel callback, exiting cleanly.");
                    return Ok(());
                }

                Ok(p2p_event) = self.p2p.poll_next() => {
                    if let Some(event) = p2p_event {
                        self.on_network_event(event);
                    }
                }
                Some(event) = self.backend.net_server.next() => {
                    match event {
                        ClientNotification::Message((from, message)) => {
                            self.on_client_message(from, message).await;
                        }
                        ClientNotification::Disconnected(from) => {
                            self.on_disconnect(from);
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
        if let NetworkEvent::MessageReceived((_, message)) = event {
            match message {
                BroadcastMessage::TrainingResult(_) => {}
                BroadcastMessage::PeerAnnouncement(_) => {}
            }
        }
    }

    fn on_disconnect(&mut self, from: ClientId) {
        self.backend.pending_clients.remove(&Client {
            id: from,
            dropping_at_end_of_round: true,
        });
    }
    async fn on_client_message(&mut self, from: ClientId, event: ClientToServerMessage) {
        match event {
            ClientToServerMessage::Join { run_id } => {
                // TODO: check whitelist
                if self.coordinator.run_id == run_id {
                    self.backend.pending_clients.insert(Client {
                        id: from.clone(),
                        dropping_at_end_of_round: false,
                    });
                    let client_joined = self
                        .backend
                        .net_server
                        .broadcast(ServerToClientMessage::P2PConnect(
                            self.p2p.get_all_peers().await,
                        ))
                        .await;
                    if let Err(e) = client_joined {
                        warn!("Error sending p2p list to client: {e}");
                    }
                } else {
                    info!("{from:?} tried to join unknown run {run_id}");
                }
            }
            ClientToServerMessage::Witness(witness) => {
                if let Err(error) = self.coordinator.witness(
                    &Client {
                        id: from,
                        dropping_at_end_of_round: false,
                    },
                    witness,
                    Self::get_timestamp(),
                ) {
                    warn!("Error when processing witness: {error}");
                }
            }
            ClientToServerMessage::HealthCheck(health_checks) => {
                match self.coordinator.health_check(
                    &Client {
                        id: from,
                        dropping_at_end_of_round: false,
                    },
                    health_checks,
                ) {
                    Ok(dropped) => info!("Dropped {} clients from health check", dropped),
                    Err(error) => warn!("Error when processing health check: {error}"),
                }
            }
            ClientToServerMessage::Checkpoint(checkpoint) => {
                if let Err(error) = self.coordinator.checkpoint(
                    &Client {
                        id: from,
                        dropping_at_end_of_round: false,
                    },
                    checkpoint,
                    Self::get_timestamp(),
                ) {
                    warn!("Error when processing checkpoint: {error}");
                }
            }
        }
        self.post_state_change();
    }

    async fn on_tick(&mut self) {
        match self.coordinator.tick(
            &self.backend,
            Self::get_timestamp(),
            rand::thread_rng().next_u64(),
        ) {
            Ok(_) | Err(CoordinatorError::Disabled) => {}
            Err(err) => warn!("Coordinator tick error: {err}"),
        }
        self.post_state_change();
        if let Err(err) = self
            .backend
            .net_server
            .broadcast(ServerToClientMessage::Coordinator(Box::new(
                self.coordinator.clone(),
            )))
            .await
        {
            warn!("Error in on_tick: {err}");
        }
        if let Some((ref sender, _)) = self.training_data_server {
            sender.send(self.coordinator.clone()).await.unwrap();
        }
    }

    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn post_state_change(&mut self) {
        if !self.coordinator.active() {
            if let Some(last_sync_step) = self.last_sync_step {
                if last_sync_step < self.coordinator.step {
                    if let Some(save_state_dir) = &self.save_state_dir {
                        let mut state = self.coordinator.clone();
                        Self::reset_ephemeral(&mut state);
                        match toml::to_string_pretty(&state) {
                            Ok(toml) => {
                                let filename = format!(
                                    "{}-step{}.toml",
                                    self.coordinator.run_id,
                                    self.coordinator.step - 1
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
                }
            }
            self.last_sync_step = Some(self.coordinator.step);
        } else {
            // reset to original values if we changed them to something special for init
            self.coordinator.warmup_time = self.original_warmup_time;
            self.coordinator.min_clients = self.original_min_clients;
        }
    }

    pub fn reset_ephemeral(coordinator: &mut Coordinator<ClientId>) {
        coordinator.run_state = RunState::WaitingForMembers;
        coordinator.clients.clear();
        coordinator.dropped_clients.clear();
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
                .map(|c| c.id.to_string())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use psyche_centralized_client::app::{AppBuilder, AppParams};
    use psyche_client::BatchShuffleType;
    use psyche_network::SecretKey;
    use tokio::{join, sync::Mutex};

    #[tokio::test]
    async fn connect_and_disconnect_nodes() {
        println!("Going to sleep...");
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("Awake!");
        let mut coordinator: Coordinator<ClientId> = Coordinator::default();

        coordinator.run_id = "test".to_string();

        let p2p_port = Some(10);

        let p2p = NC::init(
            &coordinator.run_id,
            p2p_port,
            RelayMode::Default,
            vec![],
            None,
        )
        .await
        .unwrap();

        let (tx, backend) = ChannelCoordinatorBackend::new();

        let data_server_info = DataServerInfo {
            dir: PathBuf::from("./"),
            token_size: TokenSize::TwoBytes,
            seq_len: 2048,
            shuffle_seed: [1; 32],
        };

        let server = App::new(
            false,
            coordinator,
            Some(data_server_info),
            // p2p port
            Some(1234),
            Some(8080),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let server = Arc::new(Mutex::new(server));
        let server_clone = server.clone();
        let server_task =
            tokio::spawn(async move { server_clone.lock().await.run().await.unwrap() });

        // server_task.abort();
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Client
        let client_app_params = AppParams {
            cancel: CancellationToken::default(),
            private_key: SecretKey::generate(),
            server_addr: "localhost:8080".to_string(),
            tx_tui_state: None,
            run_id: "test".to_string(),
            data_parallelism: 0,
            tensor_parallelism: 0,
            micro_batch_size: None,
            write_gradients_dir: None,
            p2p_port,
            eval_tasks: Vec::new(),
            eval_task_max_docs: None,
            checkpoint_upload_info: None,
            hub_read_token: None,
            wandb_info: None,
            batch_shuffle_type: BatchShuffleType::Fixed([0; 32]),
            optim_stats: None,
            grad_accum_in_fp32: false,
        };

        let client_app_builder = AppBuilder::new(client_app_params);

        println!("Hola 1");

        let client_handle = tokio::spawn(async { client_app_builder.run().await.unwrap() });
        // dbg!(&coordinator.clients);
        println!("Hola 2");
        //
        let _ = join!(client_handle);
        tokio::time::sleep(Duration::from_secs(1)).await;

        println!("Hola 3");
        server_task.abort();

        assert_eq!(server.lock().await.coordinator.clients.len(), 1);
    }
}
