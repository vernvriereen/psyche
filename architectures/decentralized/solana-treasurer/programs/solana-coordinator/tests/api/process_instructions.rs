use crate::api::find_pda_coordinator_instance::find_pda_coordinator_instance;

use anchor_lang::{InstructionData, ToAccountMetas};
use psyche_coordinator::{model::Model, CoordinatorConfig};
use psyche_solana_coordinator::{
    accounts::{
        InitializeCoordinatorAccounts, OwnerCoordinatorAccounts, PermissionlessCoordinatorAccounts,
    },
    instruction::{
        InitializeCoordinator, JoinRun, SetPaused, SetWhitelist, Tick, UpdateCoordinatorConfigModel,
    },
    ClientId,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    system_program,
};
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

pub async fn process_initialize_coordinator(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = InitializeCoordinatorAccounts {
        payer: payer.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: InitializeCoordinator {
            run_id: run_id.to_string(),
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}
