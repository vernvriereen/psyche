use anchor_lang::{InstructionData, ToAccountMetas};
use psyche_solana_treasurer::{accounts::CreateRunAccounts, instruction::CreateRun};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature},
    signer::Signer,
    system_program,
};
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

use crate::api::find_pda_coordinator_instance::find_pda_coordinator_instance;

pub async fn process_create_run(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);
    let accounts = CreateRunAccounts {
        payer: payer.pubkey(),
        coordinator_account: *coordinator_account,
        coordinator_instance,
        coordinator_program: psyche_solana_coordinator::ID,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: CreateRun {
            run_id: run_id.to_string(),
        }
        .data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}
