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
use psyche_coordinator::{model, Coordinator, CoordinatorConfig, HealthChecks, Witness};
use psyche_watcher::Backend as WatcherBackend;
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct SolanaBackend {
    program: Program<Arc<Keypair>>,
    cluster: Cluster,
}

pub struct SolanaBackendRunner {
    backend: SolanaBackend,
    run_id: String,
    updates: mpsc::UnboundedReceiver<RpcResponse<UiAccount>>,
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

        Ok(Self { program, cluster })
    }

    pub async fn start(self, run_id: String, coordinator: Pubkey) -> Result<SolanaBackendRunner> {
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

        Ok(SolanaBackendRunner {
            backend: self,
            updates: rx,
            run_id,
        })
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

    pub async fn update_config(
        &self,
        run_id: &str,
        config: CoordinatorConfig<solana_coordinator::ClientId>,
    ) -> Result<Signature> {
        let (instance_pda, _) = self.find_instance_from_run_id(run_id);

        let instance: solana_coordinator::CoordinatorInstance =
            self.program.account(instance_pda).await?;

        let signature = self
            .program
            .request()
            .accounts(solana_coordinator::accounts::OwnerCoordinatorAccounts {
                instance: instance_pda,
                account: instance.account,
                payer: self.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::UpdateCoordinatorConfig { config })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn set_paused(&self, run_id: &str, paused: bool) -> Result<Signature> {
        let (instance_pda, _) = self.find_instance_from_run_id(run_id);

        let instance: solana_coordinator::CoordinatorInstance =
            self.program.account(instance_pda).await?;

        let signature = self
            .program
            .request()
            .accounts(solana_coordinator::accounts::OwnerCoordinatorAccounts {
                instance: instance_pda,
                account: instance.account,
                payer: self.program.payer(),
                system_program: system_program::ID,
            })
            .args(solana_coordinator::instruction::SetPaused { paused })
            .send()
            .await?;

        Ok(signature)
    }

    #[allow(dead_code)]
    pub async fn tick(&self, run_id: &str) -> Result<Signature> {
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
            .args(solana_coordinator::instruction::Tick {})
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn witness(&self, run_id: &str, witness: Witness) -> Result<Signature> {
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
            .args(solana_coordinator::instruction::Witness {
                index: witness.index,
                proof: witness.proof,
                participant_bloom: witness.participant_bloom,
                order_bloom: witness.order_bloom,
            })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn get_coordinator_instance(
        &self,
        run_id: &str,
    ) -> Result<solana_coordinator::CoordinatorInstance> {
        let (instance_pda, _) = self.find_instance_from_run_id(run_id);

        let instance: solana_coordinator::CoordinatorInstance =
            self.program.account(instance_pda).await?;
        Ok(instance)
    }

    fn find_instance_from_run_id(&self, run_id: &str) -> (Pubkey, u8) {
        let seeds = &[
            b"coordinator",
            solana_coordinator::bytes_from_string(run_id),
        ];
        Pubkey::find_program_address(seeds, &self.program.id())
    }
}

#[async_trait::async_trait]
impl WatcherBackend<solana_coordinator::ClientId> for SolanaBackendRunner {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<solana_coordinator::ClientId>> {
        match self.updates.recv().await {
            Some(update) => match update.value.data.decode() {
                Some(data) => solana_coordinator::coordinator_account_from_bytes(&data)
                    .map_err(|_| anyhow!("Unable to decode coordinator account data"))
                    .map(|x| x.state.coordinator),
                None => bail!("Unable to decode account data"),
            },
            None => bail!("Account updates channel closed"),
        }
    }

    async fn send_witness(&mut self, witness: Witness) -> Result<()> {
        self.backend.witness(&self.run_id, witness).await?;
        Ok(())
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> Result<()> {
        unimplemented!();
    }

    async fn send_checkpoint(&mut self, _checkpoint: model::Checkpoint) -> Result<()> {
        unimplemented!();
    }
}

#[cfg(feature = "solana-localnet-tests")]
#[cfg(test)]
mod test {

    use super::*;

    use anchor_client::{
        anchor_lang::system_program,
        solana_sdk::signature::{EncodableKey, Signer},
    };
    use bytemuck::Zeroable;
    use psyche_coordinator::{CoordinatorConfig, RunState};
    use psyche_network::SecretKey;
    use rand::Rng;

    #[tokio::test]
    pub async fn localnet_coordinator_run() {
        // try to keep this and memnet_coordinator_run synced up

        let key_pair = Arc::new(
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap(),
        );
        let mut backend = SolanaBackend::new(Cluster::Localnet, key_pair.clone()).unwrap();
        let run_id = format!("{}", rand::thread_rng().gen_range(0..1000000));

        let created = backend.create_run(run_id.clone()).await.unwrap();
        let runner = backend
            .start(run_id.clone(), created.account)
            .await
            .unwrap();

        backend
            .update_config(
                &run_id,
                CoordinatorConfig::<solana_coordinator::ClientId> {
                    warmup_time: 1,
                    cooldown_time: 1,
                    max_round_train_time: 10,
                    round_witness_time: 1,
                    min_clients: 1,
                    batches_per_round: 1,
                    data_indicies_per_batch: 1,
                    verification_percent: 0,
                    witness_nodes: 0,
                    witness_quorum: 0,
                    rounds_per_epoch: 10,
                    total_steps: 100,
                    overlapped: false.into(),
                    checkpointers: FixedVec::zeroed(),
                },
            )
            .await
            .unwrap();

        let new_state = runner.wait_for_new_state().await.unwrap();
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
