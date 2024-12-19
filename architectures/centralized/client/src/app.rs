use anyhow::{Error, Result};
use bytemuck::Zeroable;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{
    CheckpointConfig, Client, ClientTUI, ClientTUIState, RunInitConfig, WandBInfo, NC,
};
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_network::{NetworkTUIState, NetworkTui, RelayMode, SecretKey, TcpClient};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::{Backend as WatcherBackend, CoordinatorTui};
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc::Sender;
use tokio::time::interval;
use tokio::{select, sync::mpsc, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

pub enum ToSend {
    Witness(Box<Witness>),
    HealthCheck(HealthChecks),
    Checkpoint(model::Checkpoint),
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
        self.tx.send(ToSend::Witness(Box::new(witness))).await?;
        Ok(())
    }

    async fn send_health_check(&mut self, health_checks: HealthChecks) -> Result<()> {
        self.tx.send(ToSend::HealthCheck(health_checks)).await?;
        Ok(())
    }

    async fn send_checkpoint(&mut self, checkpoint: model::Checkpoint) -> Result<()> {
        self.tx.send(ToSend::Checkpoint(checkpoint)).await?;
        Ok(())
    }
}

pub struct App {
    run_id: String,
    cancel: CancellationToken,
    update_tui_interval: Interval,
    tx_tui_state: Option<Sender<TabsData>>,
    coordinator_state: Coordinator<ClientId>,
    server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,
}

pub struct AppBuilder(AppParams);

pub struct AppParams {
    pub cancel: CancellationToken,
    pub private_key: SecretKey,
    pub server_addr: String,
    pub tx_tui_state: Option<Sender<TabsData>>,
    pub run_id: String,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: Option<usize>,
    pub write_gradients_dir: Option<PathBuf>,
    pub p2p_port: Option<u16>,
    pub eval_tasks: Vec<psyche_eval::Task>,
    pub eval_task_max_docs: Option<usize>,
    pub checkpoint_upload_info: Option<CheckpointConfig>,
    pub hub_read_token: Option<String>,
    pub wandb_info: Option<WandBInfo>,
    pub optim_stats: Option<u32>,
    pub grad_accum_in_fp32: bool,
}

impl AppBuilder {
    pub fn new(params: AppParams) -> Self {
        Self(params)
    }

    pub async fn build(self) -> Result<(App, NC, RunInitConfig<ClientId>)> {
        let p = self.0;

        let server_conn =
            TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
                &p.server_addr,
                p.private_key.public().into(),
                p.private_key.clone(),
            )
            .await?;

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            RelayMode::Default,
            vec![],
            Some(p.private_key.clone()),
        )
        .await?;

        let app = App {
            cancel: p.cancel,
            tx_tui_state: p.tx_tui_state,
            update_tui_interval: interval(Duration::from_millis(150)),
            coordinator_state: Coordinator::zeroed(),
            server_conn,
            run_id: p.run_id,
        };
        let state_options: RunInitConfig<ClientId> = RunInitConfig {
            data_parallelism: p.data_parallelism,
            tensor_parallelism: p.tensor_parallelism,
            micro_batch_size: p.micro_batch_size,
            write_gradients_dir: p.write_gradients_dir,
            eval_tasks: p.eval_tasks,
            eval_task_max_docs: p.eval_task_max_docs,
            checkpoint_config: p.checkpoint_upload_info,
            hub_read_token: p.hub_read_token,
            wandb_info: p.wandb_info,
            identity: p.private_key.public().into(),
            private_key: p.private_key,
            optim_stats_every_n_steps: p.optim_stats,
            grad_accum_in_fp32: p.grad_accum_in_fp32,
        };

        Ok((app, p2p, state_options))
    }
}

impl App {
    pub async fn run(&mut self, mut p2p: NC, state_options: RunInitConfig<ClientId>) -> Result<()> {
        // sanity checks
        // if let Some(CheckpointUploadInfo {
        //     hub_repo,
        //     hub_token,
        //     ..
        // }) = &self.checkpoint_upload_info
        // {
        //     let api = hf_hub::api::tokio::ApiBuilder::new()
        //         .with_token(Some(hub_token.clone()))
        //         .build()?;
        //     let repo_api = api.repo(Repo::new(hub_repo.clone(), hf_hub::RepoType::Model));
        //     if !repo_api.is_writable().await {
        //         bail!("checkpoint upload repo {hub_repo} is not writable with the passed API key.")
        //     }
        // }

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
        let (tx_from_server_message, rx_from_server_message) = mpsc::channel(128);
        let (tx_to_server_message, mut rx_to_server_message) = mpsc::channel(128);
        let mut client = Client::new(
            Backend {
                rx: rx_from_server_message,
                tx: tx_to_server_message,
            },
            p2p,
            state_options,
        );

        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                message = self.server_conn.receive() => {
                    self.on_server_message(message?, &tx_from_server_message).await;
                }
                _ = self.update_tui_interval.tick() => {
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    self.update_tui(client_tui_state, network_tui_state).await?;
                }
                res = client.finished() => {
                    res??;
                }
                Some(to_send) = rx_to_server_message.recv() => {
                    match to_send {
                        ToSend::Witness(witness) => self.server_conn.send(ClientToServerMessage::Witness(witness)).await?,
                        ToSend::HealthCheck(health_checks) => self.server_conn.send(ClientToServerMessage::HealthCheck(health_checks)).await?,
                        ToSend::Checkpoint(checkpoint) => self.server_conn.send(ClientToServerMessage::Checkpoint(checkpoint)).await?,
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
                self.coordinator_state = *state;
                let _ = tx.send(*state).await;
            }
        }
    }
}
