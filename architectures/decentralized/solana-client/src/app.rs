use crate::{backend::SolanaBackend, network_identity::NetworkIdentity};

use anchor_client::{
    solana_sdk::signature::{Keypair, Signature, Signer},
    Cluster,
};
use anyhow::{anyhow, bail, Result};
use psyche_client::{CheckpointConfig, Client, RunInitConfig, WandBInfo, NC};
use psyche_network::{RelayMode, SecretKey};
use rand::RngCore;
use std::{path::PathBuf, time::Duration};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    task::JoinHandle,
    time::{interval, Interval, MissedTickBehavior},
};
use tracing::{debug, error, info};

pub struct App {
    run_id: String,
    cluster: Cluster,
    tick_check_interval: Option<Interval>,
    // cancel: CancellationToken,
    // update_tui_interval: Interval,
    // tx_tui_state: Option<Sender<TabsData>>,
}

pub struct AppBuilder(AppParams);

pub struct AppParams {
    //pub cancel: CancellationToken,
    pub identity_secret_key: SecretKey,
    pub wallet_keypair: Arc<Keypair>,
    pub cluster: Cluster,
    pub ticker: bool,
    //pub tx_tui_state: Option<Sender<TabsData>>,
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
    pub dummy_training_delay_secs: Option<u64>,
}

impl AppBuilder {
    pub fn new(params: AppParams) -> Self {
        Self(params)
    }

    pub async fn build(
        self,
    ) -> Result<(
        App,
        NC,
        RunInitConfig<solana_coordinator::ClientId, NetworkIdentity>,
    )> {
        let p = self.0;

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            RelayMode::Default,
            vec![],
            Some(p.identity_secret_key.clone()),
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
            //cancel: p.cancel,
            //tx_tui_state: p.tx_tui_state,
            //update_tui_interval: interval(Duration::from_millis(150)),
        };
        let identity = solana_coordinator::ClientId::new(
            p.wallet_keypair.pubkey(),
            *p.identity_secret_key.public().as_bytes(),
        );
        let state_options: RunInitConfig<solana_coordinator::ClientId, NetworkIdentity> =
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
            };

        Ok((app, p2p, state_options))
    }
}

impl App {
    pub async fn run(
        &mut self,
        p2p: NC,
        state_options: RunInitConfig<solana_coordinator::ClientId, NetworkIdentity>,
    ) -> Result<()> {
        let backend =
            SolanaBackend::new(self.cluster.clone(), state_options.private_key.0.clone())?;
        let (instance_pda, instance) = backend.get_coordinator_instance(&self.run_id).await?;

        let signer = state_options.private_key.0.pubkey();
        let p2p_identity = state_options.private_key.1.public();
        let joined = backend
            .join_run(
                instance_pda,
                instance.account,
                solana_coordinator::ClientId {
                    signer,
                    p2p_identity: *p2p_identity.as_bytes(),
                },
            )
            .await?;
        info!(
            "Joined run {} from {} with transaction {}",
            self.run_id, signer, joined
        );

        let backend = backend.start(self.run_id.clone(), instance.account).await?;

        let (tick_backend, mut updates, mut latest_update) =
            match self.tick_check_interval.is_some() {
                true => {
                    let tick_backend = Arc::new(SolanaBackend::new(
                        self.cluster.clone(),
                        state_options.private_key.0.clone(),
                    )?);
                    let latest_update = tick_backend
                        .get_coordinator_account(&instance.account)
                        .await?;
                    let active_clients = latest_update
                        .state
                        .clients_state
                        .active_clients()
                        .collect::<Vec<_>>();
                    debug!("Active clients at join: {active_clients:?}");
                    let updates = backend.updates();
                    (
                        Some(tick_backend),
                        Some(updates),
                        Some(latest_update.state.coordinator),
                    )
                }
                false => (None, None, None),
            };
        let mut tick_tx: Option<JoinHandle<Result<Signature>>> = None;

        let mut client = Client::new(backend, p2p, state_options);

        loop {
            select! {
                // _ = self.cancel.cancelled() => {
                //    break;
                // }
                // _ = self.update_tui_interval.tick() => {
                //     let (client_tui_state, network_tui_state) = client.tui_states().await;
                //     self.update_tui(client_tui_state, network_tui_state).await?;
                // }
                _ = async { self.tick_check_interval.as_mut().unwrap().tick().await }, if self.tick_check_interval.is_some() && tick_tx.is_none() => {
                    let latest_update = latest_update.as_ref().unwrap();
                    let mut ticked = latest_update.clone();
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    // info!("run_state: {}, start: {}, now: {}", ticked.run_state, ticked.run_state_start_unix_timestamp, timestamp);
                    match ticked.tick(Some(vec![].iter()), timestamp, rand::thread_rng().next_u64()) {
                        Ok(_) => {
                            //if ticked.run_state != latest_update.run_state {
                                let backend = tick_backend.as_ref().unwrap().clone();
                                tick_tx = Some(tokio::spawn(async move { backend.tick(instance_pda, instance.account).await }));
                            //}
                        }
                        Err(err) => debug!("Tick simulation error: {err}")
                    };
                }
                update = async { updates.as_mut().unwrap().recv().await }, if tick_backend.is_some() => {
                    let update = match update?.value.data.decode() {
                        Some(data) => solana_coordinator::coordinator_account_from_bytes(&data)
                            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
                            .map(|x| x.state.coordinator)?,
                        None => bail!("Unable to decode account data"),
                    };
                    info!("run_state: {}, started: {}, ticked: {}", update.run_state, update.run_state_start_unix_timestamp, update.last_tick_unix_timestamp);
                    latest_update = Some(update);
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
    }

    // async fn update_tui(
    //     &mut self,
    //     client_tui_state: ClientTUIState,
    //     network_tui_state: NetworkTUIState,
    // ) -> Result<()> {
    //     if let Some(tx_tui_state) = &self.tx_tui_state {
    //         let states = (
    //             client_tui_state,
    //             (&self.coordinator_state).into(),
    //             network_tui_state,
    //             Default::default(),
    //         );
    //         tx_tui_state.send(states).await?;
    //     }
    //     Ok(())
    // }
}
