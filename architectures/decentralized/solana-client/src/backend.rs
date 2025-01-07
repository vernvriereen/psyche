use anchor_client::{
    anchor_lang::system_program,
    solana_client::{
        nonblocking::pubsub_client::PubsubClient, rpc_config::RpcAccountInfoConfig,
        rpc_response::Response as RpcResponse,
    },
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature, Signer},
        system_instruction,
    },
    Client, Cluster, Program,
};
use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use std::sync::Arc;
use tokio::sync::mpsc;

#[allow(dead_code)]
pub struct SolanaBackend {
    #[allow(unused)]
    program: Program<Arc<Keypair>>,
    cluster: Cluster,
    updates: Option<mpsc::UnboundedReceiver<RpcResponse<UiAccount>>>,
}

#[derive(Debug, Clone)]
pub struct CreatedRun {
    pub instance: Pubkey,
    pub account: Pubkey,
    pub transaction: Signature,
}

impl SolanaBackend {
    #[allow(dead_code)]
    pub fn new(cluster: Cluster, payer: Arc<Keypair>) -> Result<Self> {
        let client = Client::new(cluster.clone(), payer.clone());
        let program = client.program(solana_coordinator::ID)?;

        Ok(Self {
            program,
            cluster,
            updates: None,
        })
    }

    #[allow(dead_code)]
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
                if tx.send(update).is_err() {
                    break;
                }
            }
        });

        self.updates = Some(rx);

        Ok(())
    }

    pub async fn create_run(&self, run_id: String) -> Result<CreatedRun> {
        let coordinator_keypair = Arc::new(Keypair::new());
        let space = 8 + std::mem::size_of::<solana_coordinator::CoordinatorAccount>();
        let rent = self
            .program
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let seeds = &[
            b"coordinator",
            solana_coordinator::bytes_from_string(&run_id),
        ];
        let (instance_pda, _bump) = Pubkey::find_program_address(seeds, &self.program.id());

        let signature = self
            .program
            .request()
            .instruction(system_instruction::transfer(
                &self.program.payer(),
                &coordinator_keypair.pubkey(),
                rent,
            ))
            .instruction(system_instruction::allocate(
                &coordinator_keypair.pubkey(),
                space as u64,
            ))
            .instruction(system_instruction::assign(
                &coordinator_keypair.pubkey(),
                &self.program.id(),
            ))
            .instruction(
                self.program
                    .request()
                    .accounts(
                        solana_coordinator::accounts::InitializeCoordinatorAccounts {
                            instance: instance_pda,
                            account: coordinator_keypair.pubkey(),
                            payer: self.program.payer(),
                            system_program: system_program::ID,
                        },
                    )
                    .args(solana_coordinator::instruction::InitializeCoordinator { run_id })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .signer(coordinator_keypair.clone())
            .send()
            .await?;

        Ok(CreatedRun {
            instance: instance_pda,
            account: coordinator_keypair.pubkey(),
            transaction: signature,
        })
    }

    pub async fn set_whitelist(
        &self,
        run_id: &str,
        clients: Vec<solana_coordinator::ClientId>,
    ) -> Result<Signature> {
        let (instance_pda, _) = self.find_instance_from_run_id(run_id);

        let instance: solana_coordinator::CoordinatorInstance =
            self.program.account(instance_pda).await?;

        if instance.owner != self.program.payer() {
            bail!(
                "Not owner of run -- owner is {} and we are {}",
                instance.owner,
                self.program.payer()
            );
        }

        let signature = self
            .program
            .request()
            .accounts(solana_coordinator::accounts::OwnerCoordinatorAccounts {
                instance: instance_pda,
                account: instance.account,
                payer: self.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::SetWhitelist { clients })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn join_run(
        &self,
        run_id: &str,
        id: solana_coordinator::ClientId,
    ) -> Result<Signature> {
        let (instance_pda, _) = self.find_instance_from_run_id(run_id);

        let instance: solana_coordinator::CoordinatorInstance =
            self.program.account(instance_pda).await?;

        let signature = self
            .program
            .request()
            .accounts(
                solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    instance: instance_pda,
                    account: instance.account,
                    payer: self.program.payer(),
                    system_program: system_program::ID,
                },
            )
            .args(solana_coordinator::instruction::JoinRun { id })
            .send()
            .await?;

        Ok(signature)
    }

    pub fn find_instance_from_run_id(&self, run_id: &str) -> (Pubkey, u8) {
        let seeds = &[
            b"coordinator",
            solana_coordinator::bytes_from_string(&run_id),
        ];
        Pubkey::find_program_address(seeds, &self.program.id())
    }
}

#[async_trait::async_trait]
impl WatcherBackend<solana_coordinator::ClientId> for SolanaBackend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<solana_coordinator::ClientId>> {
        match &mut self.updates {
            Some(updates) => match updates.recv().await {
                Some(update) => match update.value.data.decode() {
                    Some(data) => solana_coordinator::coordinator_account_from_bytes(&data)
                        .map_err(|_| anyhow!("Unable to decode coordinator account data"))
                        .map(|x| x.state.coordinator),
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
        solana_sdk::signature::{EncodableKey, Signer},
    };
    use bytemuck::Zeroable;
    use psyche_coordinator::{CoodinatorConfig, RunState};
    use psyche_network::SecretKey;
    use rand::Rng;

    #[tokio::test]
    pub async fn test_create_and_initialize() {
        let key_pair = Arc::new(
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap(),
        );
        let mut backend = SolanaBackend::new(Cluster::Localnet, key_pair.clone()).unwrap();
        let run_id = format!("{}", rand::thread_rng().gen_range(0..1000000));

        let created = backend.create_run(run_id.clone()).await.unwrap();
        backend.start(created.account).await.unwrap();

        let _ = backend
            .program
            .request()
            .accounts(solana_coordinator::accounts::OwnerCoordinatorAccounts {
                instance: created.instance,
                account: created.account,
                payer: backend.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::UpdateCoordinatorConfig {
                config: CoodinatorConfig::<solana_coordinator::ClientId>::zeroed(),
            })
            .send()
            .await
            .unwrap();

        let new_state = backend.wait_for_new_state().await.unwrap();
        assert_eq!(new_state.run_state, RunState::Paused);

        let client_keypair = Arc::new(Keypair::new());
        let client_p2p = SecretKey::generate(&mut rand::rngs::OsRng);
        let client_id = solana_coordinator::ClientId::new(
            client_keypair.pubkey(),
            *client_p2p.public().as_bytes(),
        );

        // add a dummy whitelist entry so the run is permissioned
        backend
            .set_whitelist(&run_id, vec![solana_coordinator::ClientId::zeroed()])
            .await
            .unwrap();

        assert!(backend.join_run(&run_id, client_id).await.is_err());

        backend
            .set_whitelist(&run_id, vec![client_id])
            .await
            .unwrap();
    }
}
