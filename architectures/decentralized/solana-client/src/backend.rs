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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use anchor_client::{
        anchor_lang::system_program,
        solana_sdk::{
            signature::Keypair,
            signer::{EncodableKey, Signer},
        },
        Cluster,
    };

    use crate::SolanaBackend;

    #[cfg(feature = "solana-tests")]
    #[tokio::test]
    pub async fn test_set_coordinator_run_id() {
        let coordinator_keypair = Arc::new(Keypair::new());
        let key_pair =
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap();
        let backend = SolanaBackend::new(Cluster::Localnet, key_pair)
            .expect("Failed to create Solana client backend");

        // Intitilize the coordinator on-chain
        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::InitializeCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::InitializeCoordinator {})
            .signer(coordinator_keypair.clone())
            .send()
            .await
            .unwrap();

        // The coordinator has size > 52000 so we need to resize it to 55000 to be able to load it on-chain.
        // All the instructions increase the size by 10240 because is the max size for a transaction.
        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::IncreaseCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::IncreaseCoordinator { len: 20480 })
            .send()
            .await
            .unwrap();

        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::IncreaseCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::IncreaseCoordinator { len: 30720 })
            .send()
            .await
            .unwrap();

        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::IncreaseCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::IncreaseCoordinator { len: 40960 })
            .send()
            .await
            .unwrap();

        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::IncreaseCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::IncreaseCoordinator { len: 51200 })
            .send()
            .await
            .unwrap();

        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::IncreaseCoordinator {
                coordinator: coordinator_keypair.pubkey(),
                signer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::IncreaseCoordinator { len: 55000 })
            .send()
            .await
            .unwrap();

        backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::SetRunID {
                coordinator: coordinator_keypair.pubkey(),
            })
            .args(solana_coordinator::instruction::SetRunId {
                run_id: "Test".to_string(),
            })
            .send()
            .await
            .unwrap();
    }
}
