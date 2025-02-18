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
use psyche_coordinator::WitnessProof;
use psyche_core::FixedVec;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::get_accounts::get_coordinator_account_state;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_initialize;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_join_run;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_set_paused;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_set_whitelist;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_tick;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_update_config_model;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_witness;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint.process_airdrop(&payer.pubkey(), 10_000_000_000).await.unwrap();

    // Run constants
    let run_id = "Hello world!";
    let authority = Keypair::new();
    let client = Keypair::new();
    let ticker = Keypair::new();

    // create the empty pre-allocated coordinator_account
    let coordinator_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // initialize the coordinator
    process_coordinator_initialize(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account,
        run_id,
    )
    .await
    .unwrap();

    // verify that the run is in initialized state
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // update the coordinator's model
    process_coordinator_update_config_model(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account,
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
                quantize_1bit: false,
            },
        })),
    )
    .await
    .unwrap();

    // Coordinator's state should now have changed
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // add a dummy whitelist entry so the run is permissioned but no client whitelisted
    process_coordinator_set_whitelist(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account,
        run_id,
        vec![Pubkey::zeroed()],
    )
    .await
    .unwrap();

    // Generate the client key
    let client_id = ClientId::new(client.pubkey(), Default::default());

    // not whitelisted, can't join
    assert!(process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &payer,
        &coordinator_account,
        run_id,
        client_id
    )
    .await
    .is_err());

    // Add client to whitelist
    process_coordinator_set_whitelist(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account,
        run_id,
        vec![client_id.signer],
    )
    .await
    .unwrap();

    // Now whitelisted, can join
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client,
        &coordinator_account,
        run_id,
        client_id,
    )
    .await
    .unwrap();

    // Coordinator should still not be ready
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // Can't tick yet because paused
    assert!(process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_account,
        run_id
    )
    .await
    .is_err());

    // Unpause
    process_coordinator_set_paused(
        &mut endpoint,
        &payer,
        &authority,
        &coordinator_account,
        run_id,
        false,
    )
    .await
    .unwrap();

    // Coordinator should have changed
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Warmup
    );

    // Pretend 1 second passed
    endpoint.forward_clock_unix_timestamp(1).await.unwrap();

    // tick should now succeed
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_account,
        run_id,
    )
    .await
    .unwrap();

    // Coordinator in train mode
    let coordinator =
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator;
    assert_eq!(coordinator.run_state, RunState::RoundTrain);
    assert_eq!(coordinator.current_round().unwrap().height, 0);
    assert_eq!(coordinator.progress.step, 1);

    // Check that only the right user can successfully send a witness
    let witness = Witness {
        proof: WitnessProof { witness: true, position: 0, index: 0 },
        participant_bloom: Default::default(),
        order_bloom: Default::default(),
    };
    assert!(process_coordinator_witness(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_account,
        run_id,
        &witness,
    )
    .await
    .is_err());
    process_coordinator_witness(
        &mut endpoint,
        &payer,
        &client,
        &coordinator_account,
        run_id,
        &witness,
    )
    .await
    .unwrap();

    // Coordinator state after witness should change
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::RoundWitness
    );
}
