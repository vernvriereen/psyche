use anchor_lang::Discriminator;
use bytemuck::{Pod, Zeroable};
use solana_coordinator::CoordinatorAccount;
use solana_sdk::pubkey::Pubkey;
use solana_toolbox_endpoint::{ToolboxEndpoint, ToolboxEndpointError};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CoordinatorAccountWithDiscriminator {
    pub discriminator: [u8; 8],
    pub coordinator_account: CoordinatorAccount,
}

pub async fn get_coordinator_account(
    endpoint: &mut ToolboxEndpoint,
    coordinator_account: &Pubkey,
) -> Result<Option<CoordinatorAccount>, ToolboxEndpointError> {
    match endpoint
        .get_account_data_bytemuck_mapped::<CoordinatorAccountWithDiscriminator>(
            coordinator_account,
        )
        .await?
    {
        Some(coordinator_account_with_discriminator) => {
            if coordinator_account_with_discriminator.discriminator
                != CoordinatorAccount::DISCRIMINATOR
            {
                Err(ToolboxEndpointError::Custom(
                    "Invalid CoordinatorAccount discriminator",
                ))
            } else {
                Ok(Some(
                    coordinator_account_with_discriminator.coordinator_account,
                ))
            }
        }
        None => Ok(None),
    }
}
