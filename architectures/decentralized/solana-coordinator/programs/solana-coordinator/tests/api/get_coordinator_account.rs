use solana_coordinator::{coordinator_account_from_bytes, CoordinatorAccount};
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

pub async fn get_coordinator_account(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<CoordinatorAccount, ToolboxEndpointError> {
    let coordinator_bytes = endpoint.get_account_data(coordinator_account).await?;
    let coordinator_account = coordinator_account_from_bytes(&coordinator_bytes)
        .map_err(|_| ToolboxEndpointError::Custom("bytemuck error"))?;
    Ok(*coordinator_account)
}
