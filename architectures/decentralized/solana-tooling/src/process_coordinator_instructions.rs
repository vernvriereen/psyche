use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use psyche_coordinator::model::Model;
use psyche_coordinator::CoordinatorConfig;
use psyche_solana_coordinator::accounts::FreeCoordinatorAccounts;
use psyche_solana_coordinator::accounts::InitializeCoordinatorAccounts;
use psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts;
use psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts;
use psyche_solana_coordinator::find_coordinator_instance;
use psyche_solana_coordinator::instruction::FreeCoordinator;
use psyche_solana_coordinator::instruction::InitializeCoordinator;
use psyche_solana_coordinator::instruction::JoinRun;
use psyche_solana_coordinator::instruction::SetPaused;
use psyche_solana_coordinator::instruction::SetWhitelist;
use psyche_solana_coordinator::instruction::Tick;
use psyche_solana_coordinator::instruction::UpdateCoordinatorConfigModel;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::ClientId;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signature;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

pub async fn process_coordinator_initialize(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = InitializeCoordinatorAccounts {
        payer: payer.pubkey(),
        authority: authority.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: InitializeCoordinator { run_id: run_id.to_string() }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_coordinator_free(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    reimbursed: &Pubkey,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = FreeCoordinatorAccounts {
        authority: authority.pubkey(),
        reimbursed: *reimbursed,
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: FreeCoordinator {}.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_coordinator_update_config_model(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    config: Option<CoordinatorConfig<ClientId>>,
    model: Option<Model>,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: UpdateCoordinatorConfigModel { config, model }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_coordinator_set_whitelist(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    clients: Vec<Pubkey>,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetWhitelist { clients }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_coordinator_join_run(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    id: ClientId,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        user: user.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: JoinRun { id }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction_with_signers(instruction, payer, &[user]).await
}

pub async fn process_coordinator_set_paused(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    authority: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    paused: bool,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = OwnerCoordinatorAccounts {
        authority: authority.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: SetPaused { paused }.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[authority])
        .await
}

pub async fn process_coordinator_tick(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        user: user.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: Tick {}.data(),
        program_id: psyche_solana_coordinator::ID,
    };

    endpoint.process_instruction_with_signers(instruction, payer, &[user]).await
}

pub async fn process_coordinator_witness(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    user: &Keypair,
    coordinator_account: &Pubkey,
    run_id: &str,
    witness: &Witness,
) -> Result<Signature, ToolboxEndpointError> {
    let coordinator_instance = find_coordinator_instance(run_id);

    let accounts = PermissionlessCoordinatorAccounts {
        user: user.pubkey(),
        instance: coordinator_instance,
        account: *coordinator_account,
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

    endpoint.process_instruction_with_signers(instruction, payer, &[user]).await
}
