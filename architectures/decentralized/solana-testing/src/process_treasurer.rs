use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anchor_spl::associated_token;
use anchor_spl::token;
use psyche_solana_treasurer::accounts::ParticipantClaimAccounts;
use psyche_solana_treasurer::accounts::ParticipantCreateAccounts;
use psyche_solana_treasurer::accounts::RunCreateAccounts;
use psyche_solana_treasurer::accounts::RunTopUpAccounts;
use psyche_solana_treasurer::accounts::RunUpdateAccounts;
use psyche_solana_treasurer::instruction::ParticipantClaim;
use psyche_solana_treasurer::instruction::ParticipantCreate;
use psyche_solana_treasurer::instruction::RunCreate;
use psyche_solana_treasurer::instruction::RunTopUp;
use psyche_solana_treasurer::instruction::RunUpdate;
use psyche_solana_treasurer::logic::ParticipantClaimParams;
use psyche_solana_treasurer::logic::ParticipantCreateParams;
use psyche_solana_treasurer::logic::RunCreateParams;
use psyche_solana_treasurer::logic::RunTopUpParams;
use psyche_solana_treasurer::logic::RunUpdateParams;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signature;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

use crate::find_pda::find_pda_coordinator_instance;
use crate::find_pda::find_pda_participant;
use crate::find_pda::find_pda_run;

pub async fn process_treasurer_run_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    run_id: &str,
    collateral_amount_per_earned_point: u64,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let run_collateral = ToolboxEndpoint::find_spl_associated_token_account(
        &run,
        collateral_mint,
    );
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = RunCreateAccounts {
        payer: payer.pubkey(),
        authority: authority.pubkey(),
        collateral_mint: *collateral_mint,
        run,
        run_collateral,
        coordinator_instance,
        coordinator_account: *coordinator_account,
        coordinator_program: psyche_solana_coordinator::ID,
        associated_token_program: associated_token::ID,
        token_program: token::ID,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: RunCreate {
            params: RunCreateParams {
                run_id: run_id.to_string(),
                collateral_amount_per_earned_point,
            },
        }
        .data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_treasurer_run_top_up(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    authority_collateral: &Pubkey,
    collateral_mint: &Pubkey,
    run_id: &str,
    collateral_amount: u64,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let run_collateral = ToolboxEndpoint::find_spl_associated_token_account(
        &run,
        collateral_mint,
    );

    let accounts = RunTopUpAccounts {
        payer: payer.pubkey(),
        authority: authority.pubkey(),
        authority_collateral: *authority_collateral,
        collateral_mint: *collateral_mint,
        run,
        run_collateral,
        token_program: token::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: RunTopUp { params: RunTopUpParams { collateral_amount } }.data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_treasurer_run_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    params: RunUpdateParams,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = RunUpdateAccounts {
        authority: authority.pubkey(),
        run,
        coordinator_instance,
        coordinator_account: *coordinator_account,
        coordinator_program: psyche_solana_coordinator::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: RunUpdate { params }.data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_treasurer_participant_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let participant = find_pda_participant(&run, &user.pubkey());

    let accounts = ParticipantCreateAccounts {
        payer: payer.pubkey(),
        user: user.pubkey(),
        run,
        participant,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: ParticipantCreate { params: ParticipantCreateParams {} }.data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint.process_instruction_with_signers(instruction, payer, &[user]).await
}

pub async fn process_treasurer_participant_claim(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    user_collateral: &Pubkey,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    run_id: &str,
    claim_earned_points: u64,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let run_collateral = ToolboxEndpoint::find_spl_associated_token_account(
        &run,
        collateral_mint,
    );
    let participant = find_pda_participant(&run, &user.pubkey());

    let accounts = ParticipantClaimAccounts {
        payer: payer.pubkey(),
        user: user.pubkey(),
        user_collateral: *user_collateral,
        run,
        run_collateral,
        coordinator_account: *coordinator_account,
        participant,
        token_program: token::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: ParticipantClaim {
            params: ParticipantClaimParams { claim_earned_points },
        }
        .data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint.process_instruction_with_signers(instruction, payer, &[user]).await
}
