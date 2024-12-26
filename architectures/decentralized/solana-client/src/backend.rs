use anchor_client::{solana_sdk::signature::Keypair, Client, Cluster, Program};
use anyhow::Result;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_coordinator::ClientId;
use std::sync::Arc;

#[allow(dead_code)]
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
        unimplemented!();
    }

    async fn send_witness(&mut self, _witness: Witness) -> Result<()> {
        unimplemented!();
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> Result<()> {
        unimplemented!();
    }

    async fn send_checkpoint(&mut self, _checkpoint: model::Checkpoint) -> Result<()> {
        unimplemented!();
    }
}

#[cfg(test)]
mod test {

    #[cfg(feature = "solana-tests")]
    #[tokio::test]
    pub async fn test_create_and_initialize() {
        let key_pair =
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap();
        let backend = SolanaBackend::new(Cluster::Localnet, key_pair)
            .expect("Failed to create Solana client backend");

        let coordinator_keypair = Arc::new(Keypair::new());
        let space = 8 + std::mem::size_of::<CoordinatorAccount>();
        let rent = backend
            .program
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await
            .unwrap();

        let run_id = "test_run".to_string();
        let seeds = &[b"coordinator", run_id.as_bytes()];
        let (instance_pda, _bump) = Pubkey::find_program_address(seeds, &backend.program.id());

        // Build the transaction
        let tx = backend
            .program
            .request()
            .instruction(system_instruction::transfer(
                &backend.program.payer(),
                &coordinator_keypair.pubkey(),
                rent,
            ))
            .instruction(system_instruction::allocate(
                &coordinator_keypair.pubkey(),
                space as u64,
            ))
            .instruction(system_instruction::assign(
                &coordinator_keypair.pubkey(),
                &backend.program.id(),
            ))
            .instruction(
                backend
                    .program
                    .request()
                    .accounts(solana_coordinator::accounts::InitializeCoordinator {
                        instance: instance_pda,
                        coordinator: coordinator_keypair.pubkey(),
                        payer: backend.program.payer(),
                        system_program: system_program::ID,
                    })
                    .args(solana_coordinator::instruction::InitializeCoordinator { run_id })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .signer(coordinator_keypair)
            .signed_transaction()
            .await
            .expect("transaction not builts");

        let signature = backend
            .program
            .rpc()
            .send_transaction(&tx)
            .await
            .expect("transaction not sent");

        let confirmed = backend
            .program
            .rpc()
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::confirmed())
            .await
            .expect("transaction not confirmed");
    }
}
