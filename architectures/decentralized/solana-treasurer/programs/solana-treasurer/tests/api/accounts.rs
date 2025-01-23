use psyche_solana_coordinator::{coordinator_account_from_bytes, CoordinatorInstanceState};
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

pub fn find_run(run_identity: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[Run::SEED_PREFIX, run_identity],
        &psyche_solana_treasurer::ID,
    )
    .0
}

pub fn find_coordinator_instance(run_identity: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[b"coordinator", run_identity],
        &psyche_solana_coordinator::ID,
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
