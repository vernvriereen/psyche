use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_testing::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_testing::process_treasurer::process_treasurer_participant_claim;
use psyche_solana_testing::process_treasurer::process_treasurer_participant_create;
use psyche_solana_testing::process_treasurer::process_treasurer_run_create;
use psyche_solana_testing::process_treasurer::process_treasurer_run_top_up;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint.process_airdrop(&payer.pubkey(), 10_000_000_000).await.unwrap();

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
    process_treasurer_run_create(
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
    process_treasurer_run_top_up(
        &mut endpoint,
        &payer,
        &authority,
        &authority_collateral,
        &collateral_mint.pubkey(),
        run_id,
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
    process_treasurer_participant_create(&mut endpoint, &payer, &user, run_id)
        .await
        .unwrap();

    // Try claiming nothing, it should work since we earned nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        run_id,
        0,
    )
    .await
    .unwrap();

    // Claiming something while we havent earned anything should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        &collateral_mint.pubkey(),
        &coordinator_account.pubkey(),
        run_id,
        1,
    )
    .await
    .unwrap_err();

    // We should be able to to-up at any time
    process_treasurer_run_top_up(
        &mut endpoint,
        &payer,
        &authority,
        &authority_collateral,
        &collateral_mint.pubkey(),
        run_id,
        5_000_000,
    )
    .await
    .unwrap();
}
