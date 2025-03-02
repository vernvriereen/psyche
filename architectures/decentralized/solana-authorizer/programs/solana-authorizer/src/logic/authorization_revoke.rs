use anchor_lang::prelude::*;

use crate::state::Authorization;

#[derive(Accounts)]
#[instruction(params: AuthorizationRevokeParams)]
pub struct AuthorizationRevokeAccounts<'info> {
    #[account()]
    pub grantor: Signer<'info>,

    #[account(mut)]
    pub spill: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = authorization.grantor == grantor.key(),
        close = spill,
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationRevokeParams {}

pub fn authorization_revoke_processor(
    _context: Context<AuthorizationRevokeAccounts>,
    _params: AuthorizationRevokeParams,
) -> Result<()> {
    Ok(())
}
