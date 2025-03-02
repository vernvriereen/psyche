use psyche_solana_authorizer::find_authorization;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::get_accounts::get_authorization;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_delegates;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_revoke;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint.process_airdrop(&payer.pubkey(), 10_000_000_000).await.unwrap();

    // The accounts involved in our authorization
    let grantor = Keypair::new();
    let grantee = Keypair::new();
    let scope = vec![1, 2, 3, 4, 5, 6, 7];

    // Dummy delegates users
    let mut delegates = vec![];
    for _ in 0..10 {
        delegates.push(Pubkey::new_unique());
    }

    // Authorization PDA
    let authorization =
        find_authorization(&grantor.pubkey(), &grantee.pubkey(), &scope);

    // Authorization PDA doesnt exist at the start
    assert!(get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .is_none());

    // Create the authorization
    process_authorizer_authorization_create(
        &mut endpoint,
        &payer,
        &grantor,
        &grantee.pubkey(),
        &scope,
    )
    .await
    .unwrap();

    // Authorization PDA now hydrated with proper keys
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert_eq!(authorization_state.delegates, vec![]);

    // The grantee can now set the delegates
    process_authorizer_authorization_delegates(
        &mut endpoint,
        &payer,
        &grantor.pubkey(),
        &grantee,
        &scope,
        &delegates[..5],
    )
    .await
    .unwrap();

    // Authorization PDA now hydrated with proper keys and updated delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert_eq!(authorization_state.delegates, delegates[..5]);

    // The grantee can increase the set the delegates
    process_authorizer_authorization_delegates(
        &mut endpoint,
        &payer,
        &grantor.pubkey(),
        &grantee,
        &scope,
        &delegates[..],
    )
    .await
    .unwrap();

    // Authorization PDA now hydrated with proper keys and updated delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert_eq!(authorization_state.delegates, delegates[..]);

    // The grantee can decrease the set the delegates
    process_authorizer_authorization_delegates(
        &mut endpoint,
        &payer,
        &grantor.pubkey(),
        &grantee,
        &scope,
        &delegates[3..5],
    )
    .await
    .unwrap();

    // Authorization PDA now hydrated with proper keys and updated delegates
    let authorization_state = get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization_state.grantor, grantor.pubkey());
    assert_eq!(authorization_state.grantee, grantee.pubkey());
    assert_eq!(authorization_state.scope, scope);
    assert_eq!(authorization_state.delegates, delegates[3..5]);

    // The Grantor can then revoke the authorization
    process_authorizer_authorization_revoke(
        &mut endpoint,
        &payer,
        &grantor,
        &grantee.pubkey(),
        &scope,
        &payer.pubkey(),
    )
    .await
    .unwrap();

    // Authorization PDA must not exist anymore
    assert!(get_authorization(&mut endpoint, &authorization)
        .await
        .unwrap()
        .is_none());
}
