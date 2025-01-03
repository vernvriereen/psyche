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

fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..64.min(str.as_bytes().len())]
}

pub async fn process_initialize_coordinator(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator: &Pubkey,
    run_id: String,
) -> Result<Signature, ToolboxEndpointError> {
    let instance = Pubkey::find_program_address(
        &[b"coordinator", bytes_from_string(&run_id)],
        &solana_coordinator::ID,
    )
    .0;

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
