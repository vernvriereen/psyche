use psyche_solana_coordinator::{
    bytes_from_string, coordinator_account_from_bytes, CoordinatorInstance,
    CoordinatorInstanceState,
};
use psyche_solana_treasurer::run_identity_from_string;
use psyche_solana_treasurer::state::Participant;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

pub fn find_pda_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[CoordinatorInstance::SEEDS_PREFIX, bytes_from_string(run_id)],
        &psyche_solana_coordinator::ID,
    )
    .0
}

pub fn find_pda_run(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[Run::SEEDS_PREFIX, run_identity_from_string(run_id).as_ref()],
        &psyche_solana_treasurer::ID,
    )
    .0
}

pub fn find_pda_participant(run: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[Participant::SEEDS_PREFIX, run.as_ref(), user.as_ref()],
        &psyche_solana_treasurer::ID,
    )
    .0
}

pub async fn get_coordinator_instance_state(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<CoordinatorInstanceState, ToolboxEndpointError> {
    let data = endpoint
        .get_account_data(coordinator_account)
        .await?
        .ok_or_else(|| {
            ToolboxEndpointError::Custom("The coordinator account does not exist".to_string())
        })?;
    Ok(coordinator_account_from_bytes(&data)
        .map_err(|_| {
            ToolboxEndpointError::Custom("Unable to decode coordinator account data".to_string())
        })?
        .state)
}
