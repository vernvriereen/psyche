use bytemuck::Zeroable;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::ConstantLR;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::LearningRateSchedule;
use psyche_coordinator::model::Model;
use psyche_coordinator::model::Optimizer;
use psyche_coordinator::model::LLM;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::RunState;
use psyche_core::FixedVec;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_treasurer::logic::RunUpdateParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::accounts::get_coordinator_instance_state;
use crate::api::create_memnet_endpoint::create_memnet_endpoint;
use crate::api::process_instructions::process_participant_claim;
use crate::api::process_instructions::process_participant_create;
use crate::api::process_instructions::process_run_create;
use crate::api::process_instructions::process_run_top_up;
use crate::api::process_instructions::process_run_update;

#[tokio::test]
pub async fn memnet_run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint.process_airdrop(&payer.pubkey(), 10_000_000_000).await.unwrap();

    // Constants
    let run_id = "Hello world!";
    let authority = Keypair::new();

    // Prepare the collateral mint
    let collateral_mint = Keypair::new();
    let collateral_mint_authority = Keypair::new();
    endpoint
        .process_spl_token_mint_init(
            &payer,
            &collateral_mint,
            &collateral_mint_authority.pubkey(),
            None,
            6,
        )
        .await
        .unwrap();

    // create the empty pre-allocated coordinator_account
    let coordinator_account = Keypair::new();
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Create a run (it should create the underlying coordinator)
    process_run_create(
        &mut endpoint,
        &payer,
        &authority,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        run_id,
        42,
    )
    .await
    .unwrap();

    // verify that the run is in initialized state
    assert_eq!(
        get_coordinator_instance_state(
            &mut endpoint,
            &coordinator_account.pubkey()
        )
        .await
        .unwrap()
        .coordinator
        .run_state,
        RunState::Uninitialized
    );

    // Give the authority some collateral
    let authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &authority.pubkey(),
            &collateral_mint.pubkey(),
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint.pubkey(),
            &collateral_mint_authority,
            &authority_collateral,
            10_000_000,
        )
        .await
        .unwrap();

    // Fund the run with some newly minted collateral
    process_run_top_up(
        &mut endpoint,
        &payer,
        &authority,
        &authority_collateral,
        &collateral_mint.pubkey(),
        run_id,
        5_000_000,
    )
    .await
    .unwrap();

    // Create a user
    let user = Keypair::new();
    let user_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user.pubkey(),
            &collateral_mint.pubkey(),
        )
        .await
        .unwrap();

    // Create the participation manager
    process_participant_create(&mut endpoint, &payer, &user, run_id)
        .await
        .unwrap();

    // Try claiming nothing, it should work since we earned nothing
    process_participant_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        run_id,
        0,
    )
    .await
    .unwrap();

    // Claiming something while we havent earned anything should fail
    process_participant_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        run_id,
        1,
    )
    .await
    .unwrap_err();

    // Set a bunch of stuff on the run
    process_run_update(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
        RunUpdateParams {
            whitelist_clients: Some(vec![Pubkey::new_unique()]),
            paused: None,
            config: Some(CoordinatorConfig::<ClientId> {
                warmup_time: 1,
                cooldown_time: 1,
                max_round_train_time: 3,
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
            }),
            model: Some(Model::LLM(LLM {
                architecture: LLMArchitecture::HfLlama,
                checkpoint: Checkpoint::Ephemeral,
                max_seq_len: 4096,
                data_type: LLMTrainingDataType::Pretraining,
                data_location: LLMTrainingDataLocation::Local(
                    Zeroable::zeroed(),
                ),
                lr_schedule: LearningRateSchedule::Constant(
                    ConstantLR::default(),
                ),
                optimizer: Optimizer::Distro {
                    clip_grad_norm: None,
                    compression_decay: 1.0,
                    compression_decay_warmup_steps: 0,
                    compression_topk: 1,
                    compression_topk_startup: 0,
                    compression_topk_startup_steps: 0,
                    compression_chunk: 1,
                    quantize_1bit: false,
                },
            })),
            epoch_earning_rate: Some(2),
            epoch_slashing_rate: Some(0),
        },
    )
    .await
    .unwrap();

    // We should be able to to-up at any time
    process_run_top_up(
        &mut endpoint,
        &payer,
        &authority,
        &authority_collateral,
        &collateral_mint.pubkey(),
        run_id,
        5_000_000,
    )
    .await
    .unwrap();
}
