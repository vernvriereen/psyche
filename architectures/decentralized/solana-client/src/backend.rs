use anchor_client::{solana_sdk::signature::Keypair, Client, Cluster, Program};
use anyhow::Result;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_coordinator::ClientId;
use std::sync::Arc;

pub struct SolanaBackend {
    program: Program<Arc<Keypair>>,
}

impl SolanaBackend {
    pub fn new(cluster: Cluster, payer: Keypair) -> Result<Self> {
        let payer = Arc::new(payer);
        let client = Client::new(cluster, payer.clone());
        let program = client.program(solana_coordinator::ID)?;

        Ok(Self { program })
    }
}

#[async_trait::async_trait]
impl WatcherBackend<ClientId> for SolanaBackend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<ClientId>> {
        // TODO: implement
        Ok(Coordinator::default())
    }

    async fn send_witness(&mut self, _witness: Witness) -> Result<()> {
        // TODO: implement
        Ok(())
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> Result<()> {
        // TODO: implement
        Ok(())
    }

    async fn send_checkpoint(&mut self, _checkpoint: model::Checkpoint) -> Result<()> {
        // TODO: implement
        Ok(())
    }
}

impl SolanaBackend {
    pub async fn send_transacion_test(&mut self) -> Result<()> {
        let signature = self
            .program
            .request()
            .accounts(solana_coordinator::accounts::Initialize {})
            .args(solana_coordinator::instruction::Initialize)
            .send()
            .await?;
        println!("Transaction confirmed with signature: {}", signature);
        Ok(())
    }
}
