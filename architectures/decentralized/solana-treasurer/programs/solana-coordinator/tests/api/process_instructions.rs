
use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    system_program,
};
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};
use psyche_solana_treasurer::{
    accounts::InitializeCoordinatorAccounts,
    instruction::InitializeCoordinator,
};

pub async fn process_initialize_coordinator(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {

    let accounts = InitializeCoordinatorAccounts {
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: InitializeCoordinator {
            run_id: run_id.to_string(),
        }
        .data(),
        program_id: psyche_solana_treasurer::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}
