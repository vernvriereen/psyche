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
use anyhow::Context;
use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use psyche_coordinator::{
    model::{self, Model},
    CommitteeProof, Coordinator, CoordinatorConfig, HealthChecks, Witness, WitnessMetadata,
};
use psyche_watcher::Backend as WatcherBackend;
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info};

pub struct SolanaBackend {
    program_authorizer: Program<Arc<Keypair>>,
    program_coordinator: Program<Arc<Keypair>>,
    cluster: Cluster,
}

pub struct SolanaBackendRunner {
    pub(crate) backend: SolanaBackend,
    instance: Pubkey,
    account: Pubkey,
    updates: broadcast::Receiver<RpcResponse<UiAccount>>,
    init: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CreatedRun {
    pub instance: Pubkey,
    pub account: Pubkey,
    pub transaction: Signature,
}

impl SolanaBackend {
    #[allow(dead_code)]
    pub fn new(
        cluster: Cluster,
        payer: Arc<Keypair>,
        committment: CommitmentConfig,
    ) -> Result<Self> {
        let client = Client::new_with_options(cluster.clone(), payer.clone(), committment);
        let program_authorizer = client.program(psyche_solana_authorizer::ID)?;
        let program_coordinator = client.program(psyche_solana_coordinator::ID)?;
        Ok(Self {
            program_authorizer,
            program_coordinator,
            cluster,
        })
    }

    pub async fn start(
        self,
        run_id: String,
        coordinator_account: Pubkey,
    ) -> Result<SolanaBackendRunner> {
        let sub_client = PubsubClient::new(self.cluster.ws_url()).await?;
        let (tx, rx) = broadcast::channel(32);

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);

        info!("Coordinator account address: {}", coordinator_account);
        info!(
            "Coordinator instance address for run \"{}\": {}",
            run_id, coordinator_instance
        );

        let commitment = self.program_coordinator.rpc().commitment();

        let init = self
            .program_coordinator
            .rpc()
            .get_account_data(&coordinator_account)
            .await?;

        tokio::spawn(async move {
            let mut notifications = match sub_client
                .account_subscribe(
                    &coordinator_account,
                    Some(RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64Zstd),
                        commitment: Some(commitment),
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
            instance: coordinator_instance,
            account: coordinator_account,
            init: Some(init),
        })
    }

    pub async fn create_run(
        &self,
        run_id: String,
        metadata: psyche_solana_coordinator::RunMetadata,
    ) -> Result<CreatedRun> {
        let space = psyche_solana_coordinator::CoordinatorAccount::space_with_discriminator();
        let rent = self
            .program_coordinator
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let payer = self.program_coordinator.payer();
        let main_authority = self.program_coordinator.payer();
        let join_authority = self.program_coordinator.payer();

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);

        let coordinator_account_signer = Keypair::new();
        let coordinator_account = coordinator_account_signer.pubkey();

        let authorization_global = psyche_solana_authorizer::find_authorization(
            &join_authority,
            &system_program::ID,
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        );

        let signature = self
            .program_coordinator
            .request()
            .instruction(system_instruction::create_account(
                &self.program_coordinator.payer(),
                &coordinator_account,
                rent,
                space as u64,
                &self.program_coordinator.id(),
            ))
            .instruction(
                self.program_coordinator
                    .request()
                    .accounts(
                        psyche_solana_coordinator::accounts::InitCoordinatorAccounts {
                            payer,
                            coordinator_instance,
                            coordinator_account,
                            system_program: system_program::ID,
                        },
                    )
                    .args(psyche_solana_coordinator::instruction::InitCoordinator {
                        params: psyche_solana_coordinator::logic::InitCoordinatorParams {
                            main_authority,
                            join_authority,
                            run_id,
                            metadata,
                        },
                    })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .instruction(
                self.program_authorizer
                    .request()
                    .accounts(
                        psyche_solana_authorizer::accounts::AuthorizationCreateAccounts {
                            payer,
                            grantor: join_authority,
                            authorization: authorization_global,
                            system_program: system_program::ID,
                        },
                    )
                    .args(psyche_solana_authorizer::instruction::AuthorizationCreate {
                        params: psyche_solana_authorizer::logic::AuthorizationCreateParams {
                            grantee: system_program::ID,
                            scope: psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE
                                .to_vec(),
                        },
                    })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .instruction(
                self.program_authorizer
                    .request()
                    .accounts(
                        psyche_solana_authorizer::accounts::AuthorizationGrantorUpdateAccounts {
                            grantor: join_authority,
                            authorization: authorization_global,
                        },
                    )
                    .args(
                        psyche_solana_authorizer::instruction::AuthorizationGrantorUpdate {
                            params:
                                psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams {
                                    active: true,
                                },
                        },
                    )
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .signer(coordinator_account_signer)
            .send()
            .await?;

        Ok(CreatedRun {
            instance: coordinator_instance,
            account: coordinator_account,
            transaction: signature,
        })
    }

    pub async fn close_run(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::FreeCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    spill: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::FreeCoordinator {
                params: psyche_solana_coordinator::logic::FreeCoordinatorParams {},
            })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn join_run(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
    ) -> Result<Signature> {
        let coordinator_instance_state =
            self.get_coordinator_instance(&coordinator_instance).await?;
        let authorization_global = psyche_solana_authorizer::find_authorization(
            &coordinator_instance_state.join_authority,
            &system_program::ID,
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        );
        let signature = self
            .program_coordinator
            .request()
            .accounts(psyche_solana_coordinator::accounts::JoinRunAccounts {
                user: self.program_coordinator.payer(),
                authorization: authorization_global,
                coordinator_instance,
                coordinator_account,
            })
            .args(psyche_solana_coordinator::instruction::JoinRun {
                params: psyche_solana_coordinator::logic::JoinRunParams { client_id: id },
            })
            .send()
            .await?;
        Ok(signature)
    }
    pub async fn update_config_and_model(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        config: Option<CoordinatorConfig<psyche_solana_coordinator::ClientId>>,
        model: Option<Model>,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(
                psyche_solana_coordinator::instruction::UpdateCoordinatorConfigModel {
                    config,
                    model,
                },
            )
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn set_paused(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        paused: bool,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::SetPaused { paused })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn tick(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::Tick {})
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn witness(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        witness: Witness,
        metadata: WitnessMetadata,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::Witness {
                proof: witness.proof,
                participant_bloom: witness.participant_bloom,
                broadcast_bloom: witness.broadcast_bloom,
                broadcast_merkle: witness.broadcast_merkle,
                metadata,
            })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn health_check(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        check: CommitteeProof,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::HealthCheck {
                id,
                committee: check.committee,
                position: check.position,
                index: check.index,
            })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn get_coordinator_instance(
        &self,
        coordinator_instance: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorInstance> {
        let coordinator_instance_state = self
            .program_coordinator
            .account::<psyche_solana_coordinator::CoordinatorInstance>(*coordinator_instance)
            .await
            .context(format!(
                "Unable to get the coordinator_instance: {:?}",
                coordinator_instance
            ))?;
        Ok(coordinator_instance_state)
    }

    pub async fn get_coordinator_account(
        &self,
        coordinator_account: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorAccount> {
        let data = self
            .program_coordinator
            .rpc()
            .get_account_data(coordinator_account)
            .await?;
        psyche_solana_coordinator::coordinator_account_from_bytes(&data)
            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
            .copied()
    }

    pub async fn get_balance(&self, account: &Pubkey) -> Result<u64> {
        Ok(self.program_coordinator.rpc().get_balance(account).await?)
    }
}

#[async_trait::async_trait]
impl WatcherBackend<psyche_solana_coordinator::ClientId> for SolanaBackendRunner {
    async fn wait_for_new_state(
        &mut self,
    ) -> Result<Coordinator<psyche_solana_coordinator::ClientId>> {
        let data = match self.init.take() {
            Some(init) => init,
            None => match self.updates.recv().await {
                Ok(update) => match update.value.data.decode() {
                    Some(data) => data,
                    None => bail!("Unable to decode account data"),
                },
                Err(err) => bail!("Account updates channel error: {err}"),
            },
        };

        psyche_solana_coordinator::coordinator_account_from_bytes(&data)
            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
            .map(|x| {
                let update = x.state.coordinator;
                debug!("Coordinator account update, run_state={}", update.run_state);
                update
            })
    }

    async fn send_witness(&mut self, witness: Witness, metadata: WitnessMetadata) -> Result<()> {
        self.backend
            .witness(self.instance, self.account, witness, metadata)
            .await?;
        Ok(())
    }

    async fn send_health_check(
        &mut self,
        checks: HealthChecks<psyche_solana_coordinator::ClientId>,
    ) -> Result<()> {
        for (id, proof) in checks {
            self.backend
                .health_check(self.instance, self.account, id, proof)
                .await?;
        }
        Ok(())
    }

    async fn send_checkpoint(&mut self, _checkpoint: model::HubRepo) -> Result<()> {
        unimplemented!();
    }
}

impl SolanaBackendRunner {
    pub fn updates(&self) -> broadcast::Receiver<RpcResponse<UiAccount>> {
        self.updates.resubscribe()
    }
}

#[cfg(feature = "solana-localnet-tests")]
#[cfg(test)]
mod test {

    use super::*;

    use anchor_client::solana_sdk::signature::{EncodableKey, Signer};
    use bytemuck::Zeroable;
    use model::LLM;
    use psyche_coordinator::{
        model::{
            Checkpoint, ConstantLR, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType,
            LearningRateSchedule, Optimizer,
        },
        CoordinatorConfig, RunState,
    };
    use psyche_core::{FixedVec, OptimizerDefinition};
    use psyche_network::SecretKey;
    use rand::Rng;

    #[tokio::test]
    pub async fn localnet_coordinator_run() {
        // try to keep this and memnet_coordinator_run synced up

        let key_pair = Arc::new(
            Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json"))
                .unwrap(),
        );
        let backend = SolanaBackend::new(
            Cluster::Localnet,
            key_pair.clone(),
            CommitmentConfig::processed(),
        )
        .unwrap();
        let run_id = format!("{}", rand::thread_rng().gen_range(0..1000000));

        let created = backend.create_run(run_id.clone()).await.unwrap();
        let mut runner = backend
            .start(run_id.clone(), created.account)
            .await
            .unwrap();

        runner
            .backend
            .update_config_and_model(
                created.instance,
                created.account,
                Some(CoordinatorConfig::<psyche_solana_coordinator::ClientId> {
                    warmup_time: 1,
                    cooldown_time: 1,
                    max_round_train_time: 10,
                    round_witness_time: 1,
                    min_clients: 1,
                    global_batch_size: 1,
                    verification_percent: 0,
                    witness_nodes: 1,
                    rounds_per_epoch: 10,
                    total_steps: 100,
                    checkpointers: FixedVec::zeroed(),
                }),
                Some(Model::LLM(LLM {
                    architecture: LLMArchitecture::HfLlama,
                    checkpoint: Checkpoint::Ephemeral,
                    max_seq_len: 4096,
                    data_type: LLMTrainingDataType::Pretraining,
                    data_location: LLMTrainingDataLocation::Local(Zeroable::zeroed()),
                    lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
                    optimizer: OptimizerDefinition::Distro {
                        clip_grad_norm: None,
                        compression_decay: 1.0,
                        compression_decay_warmup_steps: 0,
                        compression_topk: 1,
                        compression_topk_startup: 0,
                        compression_topk_startup_steps: 0,
                        compression_chunk: 1,
                        quantize_1bit: false.into(),
                    },
                })),
            )
            .await
            .unwrap();

        let new_state = runner.wait_for_new_state().await.unwrap();
        assert_eq!(new_state.run_state, RunState::Uninitialized);

        let client_keypair = Arc::new(Keypair::new());
        let client_p2p = SecretKey::generate(&mut rand::rngs::OsRng);
        let client_id = psyche_solana_coordinator::ClientId::new(
            client_keypair.pubkey(),
            *client_p2p.public().as_bytes(),
        );
    }
}
