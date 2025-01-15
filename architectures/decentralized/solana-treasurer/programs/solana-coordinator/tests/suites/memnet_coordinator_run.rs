use crate::api::{
    create_memnet_endpoint::create_memnet_endpoint,
    get_coordinator_account::get_coordinator_account,
    process_instructions::{
        process_initialize_coordinator, process_join_run, process_set_paused,
        process_set_whitelist, process_tick, process_update_coordinator_config_model,
    },
};

use bytemuck::Zeroable;
use psyche_coordinator::{
    model::{
        Checkpoint, ConstantLR, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType,
        LearningRateSchedule, Model, Optimizer, LLM,
    },
    CoordinatorConfig, RunState,
};
use psyche_core::FixedVec;
use psyche_solana_coordinator::{ClientId, CoordinatorAccount};
use solana_sdk::{signature::Keypair, signer::Signer};

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

}
