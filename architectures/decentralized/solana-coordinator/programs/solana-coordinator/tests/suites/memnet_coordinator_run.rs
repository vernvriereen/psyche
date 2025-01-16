use crate::api::{
    accounts::get_coordinator_instance_state,
    create_memnet_endpoint::create_memnet_endpoint,
    process_instructions::{
        process_free_coordinator, process_initialize_coordinator, process_join_run,
        process_set_paused, process_set_whitelist, process_tick,
        process_update_coordinator_config_model, process_witness,
    },
};

use bytemuck::Zeroable;
use psyche_coordinator::{
    model::{
        Checkpoint, ConstantLR, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType,
        LearningRateSchedule, Model, Optimizer, LLM,
    },
    CoordinatorConfig, RunState, Witness, WitnessProof,
};
use psyche_core::FixedVec;
use solana_coordinator::{ClientId, CoordinatorAccount};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::sync::Arc;

#[tokio::test]
pub async fn memnet_coordinator_run() {
    let mut endpoint = create_memnet_endpoint().await;

    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;

    let run_id = "Hello World".to_string();
    let coordinator_account = Keypair::new();

    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &solana_coordinator::ID,
        )
        .await
        .unwrap();

    process_initialize_coordinator(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    process_update_coordinator_config_model(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        Some(CoordinatorConfig::<ClientId> {
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
        Some(Model::LLM(LLM {
            architecture: LLMArchitecture::HfLlama,
            checkpoint: Checkpoint::Ephemeral,
            max_seq_len: 4096,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::Local(Zeroable::zeroed()),
            lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
            optimizer: Optimizer::Distro {
                clip_grad_norm: None,
                compression_decay: 1.0,
                compression_decay_warmup_steps: 0,
                compression_topk: 1,
                compression_topk_startup: 0,
                compression_topk_startup_steps: 0,
                compression_chunk: 1,
                quantize: false,
            },
        })),
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // add a dummy whitelist entry so the run is permissioned
    process_set_whitelist(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        vec![Pubkey::zeroed()],
    )
    .await
    .unwrap();

    let client_keypair = Arc::new(Keypair::new());
    let client_id = ClientId::new(client_keypair.pubkey(), Default::default());

    endpoint
        .process_airdrop(&client_keypair.pubkey(), payer_lamports)
        .await
        .unwrap();

    // not whitelisted, can't join
    assert!(process_join_run(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        client_id
    )
    .await
    .is_err());

    process_set_whitelist(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        vec![client_id.signer],
    )
    .await
    .unwrap();

    process_join_run(
        &mut endpoint,
        &client_keypair,
        &coordinator_account.pubkey(),
        run_id.clone(),
        client_id,
    )
    .await
    .unwrap();

    let ticker_keypair = Arc::new(Keypair::new());
    endpoint
        .process_airdrop(&ticker_keypair.pubkey(), payer_lamports)
        .await
        .unwrap();

    // paused
    assert!(process_tick(
        &mut endpoint,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id.clone()
    )
    .await
    .is_err());

    process_set_paused(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        false,
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Warmup
    );

    endpoint.move_clock_forward(1, 1).await.unwrap();

    process_tick(
        &mut endpoint,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id.clone(),
    )
    .await
    .unwrap();

    let coordinator = get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
        .await
        .unwrap()
        .coordinator;
    assert_eq!(coordinator.run_state, RunState::RoundTrain);
    assert_eq!(coordinator.current_round().unwrap().height, 0);
    assert_eq!(coordinator.progress.step, 1);

    let witness = Witness {
        proof: WitnessProof {
            witness: true,
            position: 0,
            index: 0,
        },
        participant_bloom: Default::default(),
        order_bloom: Default::default(),
    };

    // invalid witness
    assert!(process_witness(
        &mut endpoint,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id.clone(),
        witness.clone(),
    )
    .await
    .is_err());

    process_witness(
        &mut endpoint,
        &client_keypair,
        &coordinator_account.pubkey(),
        run_id.clone(),
        witness,
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::RoundWitness
    );
}

#[tokio::test]
pub async fn memnet_coordinator_free() {
    let mut endpoint = create_memnet_endpoint().await;

    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;

    let run_id = "Free".to_string();
    let coordinator_account = Keypair::new();

    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &solana_coordinator::ID,
        )
        .await
        .unwrap();

    let start_balance = endpoint
        .get_account(&payer.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    process_initialize_coordinator(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
    )
    .await
    .unwrap();

    let next_balance = endpoint
        .get_account(&payer.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;
    assert!(next_balance < start_balance);

    process_free_coordinator(&mut endpoint, &payer, &coordinator_account.pubkey(), run_id)
        .await
        .unwrap();

    let final_balance = endpoint
        .get_account(&payer.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;
    assert!(final_balance > next_balance);

    assert!(endpoint
        .get_account(&coordinator_account.pubkey())
        .await
        .unwrap()
        .is_none());
}
