use anchor_client::{
    solana_client::{
        nonblocking::pubsub_client::PubsubClient, rpc_config::RpcAccountInfoConfig,
        rpc_response::Response as RpcResponse,
    },
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair},
    Client, Cluster, Program,
};
use anyhow::{anyhow, bail, Result};
use bytemuck::Zeroable;
use futures_util::StreamExt;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use solana_coordinator::{coordinator_account_from_bytes, ClientId};
use std::sync::Arc;
use tokio::sync::mpsc;

#[allow(dead_code)]
pub struct SolanaBackend {
    #[allow(unused)]
    program: Program<Arc<Keypair>>,
    cluster: Cluster,
    updates: Option<mpsc::UnboundedReceiver<RpcResponse<UiAccount>>>,
}

impl SolanaBackend {
    pub fn new(cluster: Cluster, payer: Arc<Keypair>) -> Result<Self> {
        let client = Client::new(cluster.clone(), payer.clone());
        let program = client.program(solana_coordinator::ID)?;

        Ok(Self {
            program,
            cluster,
            updates: None,
        })
    }

    pub async fn start(&mut self, coordinator: Pubkey) -> Result<()> {
        if self.updates.is_some() {
            bail!("Already started watching coordinator account");
        }

        let sub_client = PubsubClient::new(self.cluster.ws_url()).await?;
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut notifications = match sub_client
                .account_subscribe(
                    &coordinator,
                    Some(RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64Zstd),
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    }),
                )
                .await
            {
                Ok((notifications, _)) => notifications,
                Err(err) => {
                    tracing::error!("{}", err);
                    return;
                }
            };
            while let Some(update) = notifications.next().await {
                if let Err(_) = tx.send(update) {
                    break;
                }
            }
        });

        self.updates = Some(rx);

        Ok(())
    }
}

#[async_trait::async_trait]
impl WatcherBackend<ClientId> for SolanaBackend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<ClientId>> {
        match &mut self.updates {
            Some(updates) => match updates.recv().await {
                Some(update) => match update.value.data.decode() {
                    Some(data) => coordinator_account_from_bytes(&data)
                        .map_err(|_| anyhow!("Unable to decode coordinator account data"))
                        .map(|x| x.coordinator),
                    None => bail!("Unable to decode account data"),
                },
                None => bail!("Account updates channel closed"),
            },
            None => bail!("Not watching any coordinator account"),
        }
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

#[cfg(feature = "solana-tests")]
#[cfg(test)]
mod test {

    use super::*;

    use anchor_client::{
        anchor_lang::system_program,
        solana_client::rpc_config::RpcSendTransactionConfig,
        solana_sdk::{
            pubkey::Pubkey,
            signature::{EncodableKey, Signer},
            system_instruction,
        },
    };
    use psyche_coordinator::{CoodinatorConfig, RunState};
    use rand::Rng;

    #[tokio::test]
    pub async fn test_create_and_initialize() {
        let key_pair = Arc::new(
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap(),
        );
        let mut backend = SolanaBackend::new(Cluster::Localnet, key_pair.clone()).unwrap();

        let coordinator_keypair = Arc::new(Keypair::new());
        let space = 8 + std::mem::size_of::<solana_coordinator::CoordinatorAccount>();
        let rent = backend
            .program
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await
            .unwrap();

        let run_id = format!("{}", rand::thread_rng().gen_range(0..1000000));
        let seeds = &[
            b"coordinator",
            solana_coordinator::bytes_from_string(&run_id),
        ];
        let (instance_pda, _bump) = Pubkey::find_program_address(seeds, &backend.program.id());

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
                    .accounts(
                        solana_coordinator::accounts::InitializeCoordinatorAccounts {
                            instance: instance_pda,
                            coordinator: coordinator_keypair.pubkey(),
                            payer: backend.program.payer(),
                            system_program: system_program::ID,
                        },
                    )
                    .args(solana_coordinator::instruction::InitializeCoordinator { run_id })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .signer(coordinator_keypair.clone())
            .signed_transaction()
            .await
            .unwrap();

        let signature = backend.program.rpc().send_transaction(&tx).await.unwrap();

        let _ = backend
            .program
            .rpc()
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::processed())
            .await
            .unwrap();

        backend.start(coordinator_keypair.pubkey()).await.unwrap();

        let tx = backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::CoordinatorAccounts {
                instance: instance_pda,
                coordinator: coordinator_keypair.pubkey(),
                payer: key_pair.pubkey(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::UpdateCoordinatorConfig {
                config: CoodinatorConfig::<ClientId>::zeroed(),
            })
            .signed_transaction()
            .await
            .unwrap();

        let signature = backend
            .program
            .rpc()
            .send_transaction_with_config(
                &tx,
                RpcSendTransactionConfig {
                    skip_preflight: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let _ = backend
            .program
            .rpc()
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::confirmed())
            .await
            .unwrap();

        let new_state = backend.wait_for_new_state().await.unwrap();
        assert_eq!(new_state.run_state, RunState::Paused);
    }
}
