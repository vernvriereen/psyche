use anchor_lang::prelude::*;

use crate::state::Authorization;

#[derive(Accounts)]
#[instruction(params: AuthorizationDelegatesParams)]
pub struct AuthorizationDelegatesAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub grantee: Signer<'info>,

    #[account(
        mut,
        constraint = authorization.grantee == grantee.key(),
        realloc = Authorization::space_with_discriminator(
            authorization.scope.len(),
            params.delegates.len(),
        ),
        realloc::payer = payer,
        realloc::zero = true,
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationDelegatesParams {
    pub delegates: Vec<Pubkey>,
}

pub fn authorization_delegates_processor(
    context: Context<AuthorizationDelegatesAccounts>,
    params: AuthorizationDelegatesParams,
) -> Result<()> {
    let authorization = &mut context.accounts.authorization;
    authorization.delegates = params.delegates;
    Ok(())
}
