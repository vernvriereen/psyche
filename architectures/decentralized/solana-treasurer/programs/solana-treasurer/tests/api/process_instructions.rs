use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::associated_token;
use anchor_spl::token;
use psyche_solana_treasurer::logic::RunCreateParams;
use psyche_solana_treasurer::{accounts::RunCreateAccounts, instruction::RunCreate};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature},
    signer::Signer,
    system_program,
};
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

use crate::api::accounts::find_pda_coordinator_instance;
use crate::api::accounts::find_pda_run;

pub async fn process_run_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    collateral_mint: &Pubkey,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let run = find_pda_run(run_id);
    let run_collateral = ToolboxEndpoint::find_spl_associated_token_account(&run, collateral_mint);
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = RunCreateAccounts {
        payer: payer.pubkey(),
        authority: authority.pubkey(),
        collateral_mint: *collateral_mint,
        run,
        run_collateral,
        coordinator_account: *coordinator_account,
        coordinator_instance,
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
            },
        }
        .data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}
