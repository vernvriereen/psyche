use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_testing::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_testing::find_pda::find_pda_coordinator_instance;
use psyche_solana_testing::process_coordinator::process_coordinator_free_coordinator;
use psyche_solana_testing::process_coordinator::process_coordinator_initialize_coordinator;
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
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Initialize coordinator
    process_coordinator_initialize_coordinator(
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
    let coordinator_instance = find_pda_coordinator_instance(run_id);
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
    let reimbursed_balance_before =
        endpoint.get_account_or_default(&reimbursed).await.unwrap().lamports;

    // Free and close the coordinator account and instance
    process_coordinator_free_coordinator(
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
    let reimbursed_balance_final =
        endpoint.get_account_or_default(&reimbursed).await.unwrap().lamports;

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
