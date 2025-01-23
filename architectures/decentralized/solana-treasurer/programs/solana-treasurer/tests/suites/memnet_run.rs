use psyche_solana_coordinator::CoordinatorAccount;
use psyche_core::to_fixed_size_array;

use crate::api::accounts::get_coordinator_instance_state;
use crate::api::{
    create_memnet_endpoint::create_memnet_endpoint, process_instructions::process_run_create,
};

use solana_sdk::{signature::Keypair, signer::Signer};

use psyche_coordinator::RunState;

#[tokio::test]
pub async fn memnet_coordinator_run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .process_airdrop(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    // Constants
    let run_id = to_fixed_size_array("Hello world");
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
        &run_id,
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
}
