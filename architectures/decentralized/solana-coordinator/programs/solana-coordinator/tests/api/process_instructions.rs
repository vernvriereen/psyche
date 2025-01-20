use crate::api::find_pda_coordinator_instance::find_pda_coordinator_instance;

use anchor_lang::{InstructionData, ToAccountMetas};
use psyche_coordinator::{model::Model, CoordinatorConfig};
use psyche_solana_coordinator::{
    accounts::{
        FreeCoordinatorAccounts, InitializeCoordinatorAccounts, OwnerCoordinatorAccounts,
        PermissionlessCoordinatorAccounts,
    },
    instruction::{
        FreeCoordinator, InitializeCoordinator, JoinRun, SetPaused, SetWhitelist, Tick,
        UpdateCoordinatorConfigModel, Witness,
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

pub async fn process_free_coordinator(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = FreeCoordinatorAccounts {
        payer: payer.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: FreeCoordinator {}.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_update_coordinator_config_model(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    config: Option<CoordinatorConfig<ClientId>>,
    model: Option<Model>,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: UpdateCoordinatorConfigModel { config, model }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_set_whitelist(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    clients: Vec<Pubkey>,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetWhitelist { clients }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_join_run(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    id: ClientId,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: JoinRun { id }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_set_paused(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    paused: bool,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetPaused { paused }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_tick(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: Tick {}.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}

pub async fn process_witness(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    witness: psyche_coordinator::Witness,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_pda_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        instance: coordinator_instance,
        account: *coordinator_account,
        payer: payer.pubkey(),
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: Witness {
            proof: witness.proof,
            participant_bloom: witness.participant_bloom,
            order_bloom: witness.order_bloom,
        }
        .data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction(instruction, payer).await
}
