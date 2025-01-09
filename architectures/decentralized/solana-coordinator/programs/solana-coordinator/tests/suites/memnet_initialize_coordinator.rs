use solana_coordinator::CoordinatorAccount;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::create_memnet_endpoint::create_memnet_endpoint;
use crate::api::get_coordinator_account::get_coordinator_account;
use crate::api::process_initialize_coordinator::process_initialize_coordinator;
use crate::api::process_update_coordinator_config::process_update_coordinator_config;

#[tokio::test]
pub async fn memnet_initialize_coordinator() {
    let mut endpoint = create_memnet_endpoint().await;

    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;

    let run_id = "Hello World";
    let coordinator_account = Keypair::new();

    // Prepare the payer
    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // Create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Run the initialize IX
    process_initialize_coordinator(&mut endpoint, &payer, &coordinator_account.pubkey(), run_id)
        .await
        .unwrap();

    // Fetch the initialized coordinator account and read its config
    let config_before = get_coordinator_account(&mut endpoint, &coordinator_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .state
        .coordinator
        .config;

    // Create a slightly modified config
    let mut config_modified = config_before;
    config_modified.warmup_time = 1337;
    config_modified.max_round_train_time = 777;
    config_modified.round_witness_time = 42;
    config_modified.min_clients = 99;

    // Run the config update IX
    process_update_coordinator_config(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id,
        &config_modified,
    )
    .await
    .unwrap();

    // Re fetch the coordinator config after the IX
    let config_after = get_coordinator_account(&mut endpoint, &coordinator_account.pubkey())
        .await
        .unwrap()
        .unwrap()
        .state
        .coordinator
        .config;

    // Check that the expected values were updated
    assert_eq!(1337, config_after.warmup_time);
    assert_eq!(777, config_after.max_round_train_time);
    assert_eq!(42, config_after.round_witness_time);
    assert_eq!(99, config_after.min_clients);

    // Check that some other un-updated values are still the same
    assert_eq!(config_before.cooldown_time, config_after.cooldown_time);
    assert_eq!(config_before.witness_nodes, config_after.witness_nodes);
    assert_eq!(config_before.witness_quorum, config_after.witness_quorum);
    assert_eq!(config_before.total_steps, config_after.total_steps);
}
