use crate::api::{
    create_memnet_endpoint::create_memnet_endpoint,
    process_instructions::process_initialize_coordinator,
};

use bytemuck::Zeroable;
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


    process_initialize_coordinator(&mut endpoint, &payer, run_id).await.unwrap();

}
