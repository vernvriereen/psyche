use solana_coordinator::CoordinatorAccount;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::create_memnet_endpoint::create_memnet_endpoint;
use crate::api::process_initialize_coordinator::process_initialize_coordinator;

#[tokio::test]
pub async fn memnet_initialize_coordinator() {
    let mut endpoint = create_memnet_endpoint().await;

    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;

    let run_id = "Hello World".to_string();
    let coordinator = Keypair::new();

    // Prepare the payer
    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // Create the empty pre-allocated coordinator
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator,
            CoordinatorAccount::size_with_discriminator(),
            &solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Run the initialize IX
    process_initialize_coordinator(&mut endpoint, &payer, &coordinator.pubkey(), run_id)
        .await
        .unwrap();
}
