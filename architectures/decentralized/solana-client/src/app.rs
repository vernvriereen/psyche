use crate::{backend::SolanaBackend, network_identity::NetworkIdentity};

use anchor_client::{
    solana_sdk::signature::{Keypair, Signer},
    Cluster,
};
use anyhow::Result;
use psyche_client::{CheckpointConfig, Client, RunInitConfig, WandBInfo, NC};
use psyche_network::{allowlist, RelayMode, SecretKey};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::select;

pub struct App {
    run_id: String,
    cluster: Cluster,
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
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<solana_coordinator::ClientId, NetworkIdentity>,
    )> {
        let p = self.0;

        let allowlist = allowlist::AllowDynamic::new();

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            RelayMode::Default,
            vec![],
            Some(p.identity_secret_key.clone()),
            allowlist.clone(),
        )
        .await?;

        let app = App {
            run_id: p.run_id.clone(),
            cluster: p.cluster,
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

        Ok((app, allowlist, p2p, state_options))
    }
}

impl App {
    pub async fn run(
        &mut self,
        allowlist: allowlist::AllowDynamic,
        p2p: NC,
        state_options: RunInitConfig<solana_coordinator::ClientId, NetworkIdentity>,
    ) -> Result<()> {
        let backend =
            SolanaBackend::new(self.cluster.clone(), state_options.private_key.0.clone())?;
        let instance = backend.get_coordinator_instance(&self.run_id).await?;
        let backend = backend.start(self.run_id.clone(), instance.account).await?;

        let mut client = Client::new(backend, allowlist, p2p, state_options);

        loop {
            select! {
                // _ = self.cancel.cancelled() => {
                //    break;
                // }
                // _ = self.update_tui_interval.tick() => {
                //     let (client_tui_state, network_tui_state) = client.tui_states().await;
                //     self.update_tui(client_tui_state, network_tui_state).await?;
                // }
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
