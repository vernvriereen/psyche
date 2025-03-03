use anchor_lang::prelude::*;

use crate::state::Authorization;

#[derive(Accounts)]
#[instruction(params: AuthorizationCloseParams)]
pub struct AuthorizationCloseAccounts<'info> {
    #[account()]
    pub grantor: Signer<'info>,

    #[account(mut)]
    pub spill: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = authorization.grantor == grantor.key(),
        constraint = authorization.delegates.len() == 0,
        close = spill,
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationCloseParams {}

pub fn authorization_close_processor(
    _context: Context<AuthorizationCloseAccounts>,
    _params: AuthorizationCloseParams,
) -> Result<()> {
    Ok(())
}
