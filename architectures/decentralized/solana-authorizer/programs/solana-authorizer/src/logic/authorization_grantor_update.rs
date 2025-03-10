use anchor_lang::prelude::*;

use crate::state::Authorization;

#[derive(Accounts)]
#[instruction(params: AuthorizationGrantorUpdateParams)]
pub struct AuthorizationGrantorUpdateAccounts<'info> {
    #[account()]
    pub grantor: Signer<'info>,

    #[account(
        mut,
        constraint = authorization.grantor == grantor.key(),
    )]
    pub authorization: Box<Account<'info, Authorization>>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationGrantorUpdateParams {
    pub active: bool,
}

pub fn authorization_grantor_update_processor(
    context: Context<AuthorizationGrantorUpdateAccounts>,
    params: AuthorizationGrantorUpdateParams,
) -> Result<()> {
    let authorization = &mut context.accounts.authorization;
    authorization.active = params.active;
    authorization.grantor_update_unix_timestamp = Clock::get()?.unix_timestamp;
    Ok(())
}
