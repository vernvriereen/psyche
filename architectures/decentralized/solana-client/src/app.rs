use crate::{backend::SolanaBackend, network_identity::NetworkIdentity};

use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::{Keypair, Signature, Signer},
    },
    Cluster,
};
use anyhow::{anyhow, bail, Result};
use psyche_client::{
    CheckpointConfig, Client, ClientTUI, ClientTUIState, RunInitConfig, WandBInfo, NC,
};
use psyche_coordinator::{Coordinator, RunState};
use psyche_network::{allowlist, Compression, DiscoveryMode, NetworkTUIState, NetworkTui, RelayMode, SecretKey};
use psyche_tui::{logging::LoggerWidget, CustomWidget, TabbedWidget};
use psyche_watcher::CoordinatorTui;
use rand::RngCore;
use std::{path::PathBuf, time::Duration};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::mpsc::Sender,
    task::JoinHandle,
    time::{interval, Interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

pub struct App {
    run_id: String,
    cluster: Cluster,
    tick_check_interval: Option<Interval>,
    cancel: CancellationToken,
    update_tui_interval: Interval,
    tx_tui_state: Option<Sender<TabsData>>,
}

pub struct AppBuilder(AppParams);

pub struct AppParams {
    pub cancel: CancellationToken,
    pub identity_secret_key: SecretKey,
    pub wallet_keypair: Arc<Keypair>,
    pub cluster: Cluster,
    pub ticker: bool,
    pub tx_tui_state: Option<Sender<TabsData>>,
    pub run_id: String,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: Option<usize>,
    pub write_gradients_dir: Option<PathBuf>,
    pub p2p_port: Option<u16>,
    pub p2p_interface: Option<String>,
    pub eval_tasks: Vec<psyche_eval::Task>,
    pub eval_task_max_docs: Option<usize>,
    pub checkpoint_upload_info: Option<CheckpointConfig>,
    pub hub_read_token: Option<String>,
    pub wandb_info: Option<WandBInfo>,
    pub optim_stats: Option<u32>,
    pub grad_accum_in_fp32: bool,
    pub dummy_training_delay_secs: Option<u64>,
    pub max_concurrent_parameter_requests: usize,
    pub compression: u32
}

impl AppBuilder {
    pub fn new(params: AppParams) -> Self {
        Self(params)
    }

    pub async fn build(
        self,
    ) -> Result<(
        App,
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity>,
    )> {
        let p = self.0;

        let allowlist = allowlist::AllowDynamic::new();

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            p.p2p_interface,
            RelayMode::Default,
            DiscoveryMode::N0,
            vec![],
            Some(p.identity_secret_key.clone()),
            allowlist.clone(),
        )
        .await?;

        let app = App {
            run_id: p.run_id.clone(),
            cluster: p.cluster,
            tick_check_interval: match p.ticker {
                true => {
                    let mut interval = interval(Duration::from_millis(500));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    Some(interval)
                }
                false => None,
            },
            cancel: p.cancel,
            tx_tui_state: p.tx_tui_state,
            update_tui_interval: interval(Duration::from_millis(150)),
        };
        let identity = psyche_solana_coordinator::ClientId::new(
            p.wallet_keypair.pubkey(),
            *p.identity_secret_key.public().as_bytes(),
        );
        let state_options: RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity> =
            RunInitConfig {
                data_parallelism: p.data_parallelism,
                tensor_parallelism: p.tensor_parallelism,
                micro_batch_size: p.micro_batch_size,
                write_gradients_dir: p.write_gradients_dir,
                eval_tasks: p.eval_tasks,
                eval_task_max_docs: p.eval_task_max_docs,
                checkpoint_config: p.checkpoint_upload_info,
                hub_read_token: p.hub_read_token,
                wandb_info: p.wandb_info,
                identity,
                network_identity: identity.into(),
                private_key: (p.wallet_keypair.clone(), p.identity_secret_key),
                optim_stats_every_n_steps: p.optim_stats,
                grad_accum_in_fp32: p.grad_accum_in_fp32,
                dummy_training_delay_secs: p.dummy_training_delay_secs,
                max_concurrent_parameter_requests: p.max_concurrent_parameter_requests,
                distro_compression: Compression::new(p.compression)
            };

        Ok((app, allowlist, p2p, state_options))
    }
}

impl App {
    pub async fn run(
        &mut self,
        allowlist: allowlist::AllowDynamic,
        p2p: NC,
        state_options: RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity>,
    ) -> Result<()> {
        let backend = SolanaBackend::new(
            self.cluster.clone(),
            state_options.private_key.0.clone(),
            CommitmentConfig::confirmed(),
        )?;
        let coordinator_instance =
            psyche_solana_coordinator::find_coordinator_instance(&self.run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;

        let coordinator_account = coordinator_instance_state.coordinator_account;

        let backend_runner = backend
            .start(self.run_id.clone(), coordinator_account)
            .await?;

        let backend = Arc::new(SolanaBackend::new(
            self.cluster.clone(),
            state_options.private_key.0.clone(),
            CommitmentConfig::confirmed(),
        )?);
        let signer = state_options.private_key.0.pubkey();
        let p2p_identity = state_options.private_key.1.public();

        let current_coordinator_state = backend
            .get_coordinator_account(&coordinator_account)
            .await?
            .state
            .coordinator;

        let mut already_joined_next_run: bool;
        if current_coordinator_state.run_state == RunState::WaitingForMembers {
            let joined = backend
                .join_run(
                    coordinator_instance,
                    coordinator_account,
                    psyche_solana_coordinator::ClientId {
                        signer,
                        p2p_identity: *p2p_identity.as_bytes(),
                    },
                )
                .await?;
            info!(
                "Joined run {} from {} with transaction {}",
                self.run_id, signer, joined
            );
            already_joined_next_run = true;
        } else {
            info!("Waiting for the current epoch to end before joining.");
            already_joined_next_run = false;
        }

        // Update the latest update after joining the run to advance the state.
        let coordinator_state = backend
            .get_coordinator_account(&coordinator_account)
            .await?
            .state;
        let mut latest_update = coordinator_state.coordinator;
        let mut updates = backend_runner.updates();
        let mut tick_tx: Option<JoinHandle<Result<Signature>>> = None;
        let mut client = Client::new(backend_runner, allowlist, p2p, state_options);

        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                _ = self.update_tui_interval.tick() => {
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    self.update_tui(client_tui_state, &latest_update, network_tui_state).await?;
                }
                _ = async { self.tick_check_interval.as_mut().unwrap().tick().await }, if self.tick_check_interval.is_some() && tick_tx.is_none() => {
                    let mut ticked = latest_update;
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    let pending_clients = (ticked.run_state == RunState::WaitingForMembers).then(|| coordinator_state.get_active_clients());

                    match ticked.tick(pending_clients, timestamp, rand::thread_rng().next_u64()) {
                        Ok(_) => {
                            if ticked.run_state != latest_update.run_state {
                                let backend = backend.clone();
                                let backend_clone = backend.clone();
                                tick_tx = Some(tokio::spawn(async move { backend.tick(coordinator_instance, coordinator_account).await }));
                                // This means the epoch finished so we're rejoining the run to participate in the next one.
                                if ticked.run_state == RunState::WaitingForMembers && latest_update.run_state == RunState::Cooldown && !already_joined_next_run {
                                    let joined = backend_clone
                                        .join_run(
                                            coordinator_instance,
                                            coordinator_account,
                                            psyche_solana_coordinator::ClientId {
                                                signer,
                                                p2p_identity: *p2p_identity.as_bytes(),
                                            },
                                        )
                                        .await?;
                                    info!(
                                        "Joined run for next epoch {} from {} with transaction {}",
                                        self.run_id, signer, joined
                                    );
                                    already_joined_next_run = true;
                                } else if ticked.run_state == RunState::RoundTrain && latest_update.run_state == RunState::Warmup {
                                    already_joined_next_run = false;
                                }
                            }
                        }
                        Err(err) => debug!("Tick simulation error: {err}")
                    };
                }
                update = async { updates.recv().await } => {
                    let update = match update?.value.data.decode() {
                        Some(data) => psyche_solana_coordinator::coordinator_account_from_bytes(&data)
                            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
                            .map(|x| x.state.coordinator)?,
                        None => bail!("Unable to decode account data"),
                    };
                    latest_update = update;
                }
                tx = async { tick_tx.as_mut().unwrap().await }, if tick_tx.is_some() => {
                    tick_tx = None;
                    match tx? {
                        Ok(signature) => info!("Tick transaction {}", signature),
                        Err(err) => error!("Tick transaction error: {}", err)
                    };
                }
                res = client.finished() => {
                    res??;
                }

            }
        }

        Ok(())
    }

    async fn update_tui(
        &mut self,
        client_tui_state: ClientTUIState,
        coordinator_state: &Coordinator<psyche_solana_coordinator::ClientId>,
        network_tui_state: NetworkTUIState,
    ) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                client_tui_state,
                coordinator_state.into(),
                network_tui_state,
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }
}
