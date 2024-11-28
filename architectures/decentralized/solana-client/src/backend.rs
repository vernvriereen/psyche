use std::str::FromStr;

use anyhow::Result;
use psyche_client::ClientId;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer,
    transaction::Transaction,
};

pub struct SolanaBackend {
    client: solana_client::rpc_client::RpcClient,
    payer: Keypair,
    program_id: solana_sdk::pubkey::Pubkey,
}

impl SolanaBackend {
    pub fn new(url: String, program_id: String, payer: Keypair) -> Result<Self> {
        let client = RpcClient::new(url);
        let program_id = Pubkey::from_str(&program_id)?;

        Ok(Self {
            client,
            payer,
            program_id,
        })
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
        let accounts = vec![solana_sdk::instruction::AccountMeta::new_readonly(
            self.payer.pubkey(),
            true,
        )];

        let data =
            anchor_lang::InstructionData::data(&solana_coordinator::instruction::Initialize {});
        let initialize_instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data,
        };

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &[initialize_instruction],
            Some(&self.payer.pubkey()),
            &[&self.payer],
            recent_blockhash,
        );
        let signature = self.client.send_and_confirm_transaction(&transaction)?;

        println!("Transaction confirmed with signature: {}", signature);
        Ok(())
    }
}
