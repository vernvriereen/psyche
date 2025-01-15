use anchor_lang::Discriminator;
use bytemuck::{Pod, Zeroable};
use psyche_solana_coordinator::CoordinatorAccount;
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
    endpoint
        .get_account_data_bytemuck_mapped::<CoordinatorAccountWithDiscriminator>(
            coordinator_account,
        )
        .await?
        .map(|coordinator_account_with_discriminator| {
            if coordinator_account_with_discriminator.discriminator
                != CoordinatorAccount::DISCRIMINATOR
            {
                return Err(ToolboxEndpointError::Custom(
                    "Invalid CoordinatorAccount discriminator",
                ));
            }
            Ok(coordinator_account_with_discriminator.coordinator_account)
        })
        .transpose()
}
