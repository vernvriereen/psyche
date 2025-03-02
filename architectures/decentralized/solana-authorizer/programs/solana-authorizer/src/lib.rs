pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("AJ911Ut5zBuWmaeMpCfcnV9jnBnMRnHwaHf6DqUZzE4L");

#[program]
pub mod psyche_solana_authorizer {
    use super::*;

    pub fn authorization_create(
        context: Context<AuthorizationCreateAccounts>,
        params: AuthorizationCreateParams,
    ) -> Result<()> {
        authorization_create_processor(context, params)
    }

    pub fn authorization_delegates(
        context: Context<AuthorizationDelegatesAccounts>,
        params: AuthorizationDelegatesParams,
    ) -> Result<()> {
        authorization_delegates_processor(context, params)
    }

    pub fn authorization_revoke(
        context: Context<AuthorizationRevokeAccounts>,
        params: AuthorizationRevokeParams,
    ) -> Result<()> {
        authorization_revoke_processor(context, params)
    }
}

pub fn find_authorization(
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Authorization::SEEDS_PREFIX,
            grantor.as_ref(),
            grantee.as_ref(),
            scope,
        ],
        &crate::ID,
    )
    .0
}
