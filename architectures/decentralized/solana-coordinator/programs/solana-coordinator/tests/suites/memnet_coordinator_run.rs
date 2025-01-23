use crate::api::{
    accounts::{find_coordinator_instance, get_coordinator_instance_state},
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
use psyche_solana_coordinator::{ClientId, CoordinatorAccount};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

#[tokio::test]
pub async fn memnet_coordinator_run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .process_airdrop(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    // Run constants
    let run_id = "Hello World";
    let coordinator_account = Keypair::new();

    // The owner authority of the run
    let authority = Keypair::new();

    // create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // initialize the coordinator
    process_initialize_coordinator(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
    )
    .await
    .unwrap();

    // verify that the run is in initialized state
    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // update the coordinator's model
    process_update_coordinator_config_model(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
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

    // Coordinator's state should now have changed
    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // add a dummy whitelist entry so the run is permissioned but no client whitelisted
    process_set_whitelist(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
        vec![Pubkey::zeroed()],
    )
    .await
    .unwrap();

    // Generate the client key and fund it
    let client_keypair = Keypair::new();
    let client_id = ClientId::new(client_keypair.pubkey(), Default::default());

    // not whitelisted, can't join
    assert!(process_join_run(
        &mut endpoint,
        &payer,
        &payer,
        &coordinator_account.pubkey(),
        run_id,
        client_id
    )
    .await
    .is_err());

    // Add client to whitelist
    process_set_whitelist(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
        vec![client_id.signer],
    )
    .await
    .unwrap();

    // Now whitelisted, can join
    process_join_run(
        &mut endpoint,
        &payer,
        &client_keypair,
        &coordinator_account.pubkey(),
        run_id,
        client_id,
    )
    .await
    .unwrap();

    // Create a ticker key and fund it
    let ticker_keypair = Keypair::new();

    // Can't tick yet because paused
    assert!(process_tick(
        &mut endpoint,
        &payer,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id
    )
    .await
    .is_err());

    // Unpause
    process_set_paused(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
        false,
    )
    .await
    .unwrap();

    // Coordinator should have changed
    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Warmup
    );

    endpoint.move_clock_forward(1, 1).await.unwrap();

    // tick should now succeed
    process_tick(
        &mut endpoint,
        &payer,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id,
    )
    .await
    .unwrap();

    // Coordinator in train mode
    let coordinator = get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
        .await
        .unwrap()
        .coordinator;
    assert_eq!(coordinator.run_state, RunState::RoundTrain);
    assert_eq!(coordinator.current_round().unwrap().height, 0);
    assert_eq!(coordinator.progress.step, 1);

    // Check that only the right user can successfully send a witness
    let witness = Witness {
        proof: WitnessProof {
            witness: true,
            position: 0,
            index: 0,
        },
        participant_bloom: Default::default(),
        order_bloom: Default::default(),
    };
    assert!(process_witness(
        &mut endpoint,
        &payer,
        &ticker_keypair,
        &coordinator_account.pubkey(),
        run_id,
        witness.clone(),
    )
    .await
    .is_err());
    process_witness(
        &mut endpoint,
        &payer,
        &client_keypair,
        &coordinator_account.pubkey(),
        run_id,
        witness,
    )
    .await
    .unwrap();

    // Coordinator state after witness should change
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

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .process_airdrop(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    // Run constants
    let run_id = "Hello World";
    let coordinator_account = Keypair::new();

    // The owner authority of the run
    let authority = Keypair::new();

    // Check the payer and authority balance before paying for the coordinator
    let payer_balance_start = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let authority_balance_start = endpoint
        .get_account_or_default(&authority.pubkey())
        .await
        .unwrap()
        .lamports;

    // create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Initialize coordinator
    process_initialize_coordinator(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account.pubkey(),
        run_id,
    )
    .await
    .unwrap();

    // Check the payer and authority balance after paying for the coordinator accounts
    let payer_balance_after = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let authority_balance_after = endpoint
        .get_account_or_default(&authority.pubkey())
        .await
        .unwrap()
        .lamports;

    // Check that balance mouvements match what we expect
    assert!(payer_balance_after < payer_balance_start);
    assert_eq!(authority_balance_after, authority_balance_start);

    // Check that the coordinator instance and account do actually exists now
    let coordinator_instance = find_coordinator_instance(&run_id);
    assert!(endpoint
        .get_account(&coordinator_account.pubkey())
        .await
        .unwrap()
        .is_some());
    assert!(endpoint
        .get_account(&coordinator_instance)
        .await
        .unwrap()
        .is_some());

    // This account will be reimbursed for the costs of the rent
    let reimbursed = Pubkey::new_unique();
    let reimbursed_balance_before = endpoint
        .get_account_or_default(&reimbursed)
        .await
        .unwrap()
        .lamports;

    // Free and close the coordinator account and instance
    process_free_coordinator(
        &mut endpoint,
        &payer,
        &authority,
        &reimbursed,
        &coordinator_account.pubkey(),
        run_id,
    )
    .await
    .unwrap();

    // Check all the keys balances at the end
    let payer_balance_final = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let authority_balance_final = endpoint
        .get_account_or_default(&authority.pubkey())
        .await
        .unwrap()
        .lamports;
    let reimbursed_balance_final = endpoint
        .get_account_or_default(&reimbursed)
        .await
        .unwrap()
        .lamports;

    // Check that we did in fact get reimbursed to the proper account
    assert_eq!(payer_balance_after - 5_000 * 2, payer_balance_final);
    assert_eq!(authority_balance_after, authority_balance_final);
    assert!(reimbursed_balance_before < reimbursed_balance_final);

    // Check that the coordinator account and instances were actually closed
    assert!(endpoint
        .get_account(&coordinator_account.pubkey())
        .await
        .unwrap()
        .is_none());
    assert!(endpoint
        .get_account(&coordinator_instance)
        .await
        .unwrap()
        .is_none());
}
