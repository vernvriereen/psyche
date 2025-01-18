use psyche_solana_coordinator::CoordinatorAccount;

use crate::api::{
    create_memnet_endpoint::create_memnet_endpoint, process_instructions::process_create_run,
};
use crate::api::get_coordinator_instance_state::get_coordinator_instance_state;

use solana_sdk::{signature::Keypair, signer::Signer};

use psyche_coordinator::{
    model::{
        Checkpoint, ConstantLR, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType,
        LearningRateSchedule, Model, Optimizer, LLM,
    },
    CoordinatorConfig, RunState,
};

#[tokio::test]
pub async fn memnet_coordinator_run() {
    let mut endpoint = create_memnet_endpoint().await;

    let run_id = "Hello World";

    // Create payer key and fund it
    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;
    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // create the empty pre-allocated coordinator_account
    let coordinator_account = Keypair::new();
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Create a run (it should create the underlying coordinator)
    process_create_run(&mut endpoint, &payer, &coordinator_account.pubkey(), run_id)
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
}
