use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use solana_coordinator::accounts::InitializeCoordinatorAccounts;
use solana_coordinator::instruction::InitializeCoordinator;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signature;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

use crate::api::find_pda_instance::find_pda_instance;

pub async fn process_initialize_coordinator(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator: &Pubkey,
    run_id: String,
) -> Result<Signature, ToolboxEndpointError> {
    let instance = find_pda_instance(&run_id);

    let accounts = InitializeCoordinatorAccounts {
        payer: payer.pubkey(),
        coordinator: *coordinator,
        instance,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: InitializeCoordinator { run_id }.data(),
        program_id: solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, &payer).await
}
