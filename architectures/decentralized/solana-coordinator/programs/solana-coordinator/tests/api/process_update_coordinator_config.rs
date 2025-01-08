use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use psyche_coordinator::CoodinatorConfig;
use solana_coordinator::accounts::CoordinatorAccounts;
use solana_coordinator::instruction::UpdateCoordinatorConfig;
use solana_coordinator::ClientId;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signature;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

use crate::api::find_pda_coordinator_instance::find_pda_coordinator_instance;

pub async fn process_update_coordinator_config(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &String,
    config: &CoodinatorConfig<ClientId>,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = CoordinatorAccounts {
        payer: payer.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: UpdateCoordinatorConfig { config: *config }.data(),
        program_id: solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, &payer).await
}
