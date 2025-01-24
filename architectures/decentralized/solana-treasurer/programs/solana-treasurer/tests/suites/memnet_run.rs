use psyche_solana_coordinator::CoordinatorAccount;

use crate::api::accounts::get_coordinator_instance_state;
use crate::api::process_instructions::process_participant_claim;
use crate::api::process_instructions::process_participant_create;
use crate::api::{
    create_memnet_endpoint::create_memnet_endpoint, process_instructions::process_run_create,
    process_instructions::process_run_fund,
};
use psyche_coordinator::RunState;
use solana_sdk::{signature::Keypair, signer::Signer};

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
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
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
    process_run_fund(
        &mut endpoint,
        &payer,
        &authority,
        &authority_collateral,
        &collateral_mint.pubkey(),
        &run_id,
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
    process_participant_create(&mut endpoint, &payer, &user, &run_id)
        .await
        .unwrap();

    process_participant_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        &run_id,
        0,
    )
    .await
    .unwrap();
}
