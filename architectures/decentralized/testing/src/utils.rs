use std::sync::Arc;

use anchor_client::{
    solana_sdk::{pubkey::Pubkey, signature::Keypair},
    Cluster, Program,
};
use psyche_coordinator::{Round, RunState, NUM_STORED_ROUNDS};
use psyche_core::FixedVec;
use psyche_solana_coordinator::SOLANA_MAX_NUM_PENDING_CLIENTS;

pub struct SolanaTestClient {
    program: Program<Arc<Keypair>>,
    account: Pubkey,
}

impl SolanaTestClient {
    pub async fn new(run_id: String) -> Self {
        let key_pair = Arc::new(Keypair::new());
        let cluster = Cluster::Localnet;
        let client = anchor_client::Client::new(cluster.clone(), key_pair.clone());
        let program = client.program(psyche_solana_coordinator::ID).unwrap();
        let seeds = &[
            psyche_solana_coordinator::CoordinatorInstance::SEEDS_PREFIX,
            psyche_solana_coordinator::bytes_from_string(&run_id),
        ];
        let (account, _) = Pubkey::find_program_address(seeds, &program.id());
        let instance: psyche_solana_coordinator::CoordinatorInstance =
            program.account(account).await.unwrap();
        Self {
            program,
            account: instance.account,
        }
    }

    async fn get_coordinator_account(&self) -> psyche_solana_coordinator::CoordinatorAccount {
        let data = self
            .program
            .rpc()
            .get_account_data(&self.account)
            .await
            .unwrap();
        *psyche_solana_coordinator::coordinator_account_from_bytes(&data).unwrap()
    }

    pub async fn get_clients(
        &self,
    ) -> FixedVec<psyche_solana_coordinator::Client, SOLANA_MAX_NUM_PENDING_CLIENTS> {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.clients_state.clients
    }

    pub async fn get_clients_len(&self) -> usize {
        let clients = self.get_clients().await;
        clients.len()
    }

    pub async fn get_run_state(&self) -> RunState {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.run_state
    }

    pub async fn get_rounds(&self) -> [Round; NUM_STORED_ROUNDS] {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.epoch_state.rounds
    }

    pub async fn get_rounds_head(&self) -> u32 {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.epoch_state.rounds_head
    }

    pub async fn get_current_epoch(&self) -> u16 {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.progress.epoch
    }
}
