use psyche_solana_coordinator::{
    bytes_from_string, coordinator_account_from_bytes, CoordinatorInstanceState,
};
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

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

pub fn find_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[b"coordinator", bytes_from_string(run_id)],
        &psyche_solana_coordinator::ID,
    )
    .0
}
