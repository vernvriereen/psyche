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

    pub fn authorization_grantor_update(
        context: Context<AuthorizationGrantorUpdateAccounts>,
        params: AuthorizationGrantorUpdateParams,
    ) -> Result<()> {
        authorization_grantor_update_processor(context, params)
    }

    pub fn authorization_grantee_update(
        context: Context<AuthorizationGranteeUpdateAccounts>,
        params: AuthorizationGranteeUpdateParams,
    ) -> Result<()> {
        authorization_grantee_update_processor(context, params)
    }

    pub fn authorization_close(
        context: Context<AuthorizationCloseAccounts>,
        params: AuthorizationCloseParams,
    ) -> Result<()> {
        authorization_close_processor(context, params)
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
