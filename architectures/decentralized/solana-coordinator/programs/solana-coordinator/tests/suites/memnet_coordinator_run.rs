use std::sync::Arc;

use crate::api::{
    accounts::get_coordinator_instance_state,
    create_memnet_endpoint::create_memnet_endpoint,
    process_instructions::{
        process_initialize_coordinator, process_join_run, process_set_whitelist,
        process_update_coordinator_config,
    },
};

use bytemuck::Zeroable;
use psyche_coordinator::{CoordinatorConfig, RunState};
use solana_coordinator::{ClientId, CoordinatorAccount};
use solana_sdk::{signature::Keypair, signer::Signer};

#[tokio::test]
pub async fn memnet_coordinator_run() {
    let mut endpoint = create_memnet_endpoint().await;

    let payer = Keypair::new();
    let payer_lamports = 10_000_000_000;

    let run_id = "Hello World".to_string();
    let coordinator_account = Keypair::new();

    endpoint
        .process_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // create the empty pre-allocated coordinator_account
    endpoint
        .process_system_create_exempt(
            &payer,
            &coordinator_account,
            CoordinatorAccount::size_with_discriminator(),
            &solana_coordinator::ID,
        )
        .await
        .unwrap();

    process_initialize_coordinator(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    process_update_coordinator_config(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        CoordinatorConfig::<ClientId>::zeroed(),
    )
    .await
    .unwrap();

    assert_eq!(
        get_coordinator_instance_state(&mut endpoint, &coordinator_account.pubkey())
            .await
            .unwrap()
            .coordinator
            .run_state,
        RunState::Paused
    );

    // add a dummy whitelist entry so the run is permissioned
    process_set_whitelist(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        vec![ClientId::zeroed()],
    )
    .await
    .unwrap();

    let client_keypair = Arc::new(Keypair::new());
    let client_id = ClientId::new(client_keypair.pubkey(), Default::default());

    endpoint
        .process_airdrop(&client_keypair.pubkey(), payer_lamports)
        .await
        .unwrap();

    // not whitelisted, can't join
    assert!(process_join_run(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        client_id
    )
    .await
    .is_err());

    process_set_whitelist(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        vec![client_id],
    )
    .await
    .unwrap();

    process_join_run(
        &mut endpoint,
        &payer,
        &coordinator_account.pubkey(),
        run_id.clone(),
        client_id,
    )
    .await
    .unwrap();
}
