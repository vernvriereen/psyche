use anchor_lang::prelude::*;

use crate::state::Authorization;

#[derive(Accounts)]
#[instruction(params: AuthorizationCreateParams)]
pub struct AuthorizationCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub grantor: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Authorization::space_with_discriminator(
            params.scope.len(),
            0
        ),
        seeds = [
            Authorization::SEEDS_PREFIX,
            grantor.key().as_ref(),
            params.grantee.as_ref(),
            params.scope.as_ref(),
        ],
        bump,
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationCreateParams {
    pub grantee: Pubkey,
    pub scope: Vec<u8>,
}

pub fn authorization_create_processor(
    context: Context<AuthorizationCreateAccounts>,
    params: AuthorizationCreateParams,
) -> Result<()> {
    let authorization = &mut context.accounts.authorization;
    authorization.bump = context.bumps.authorization;
    authorization.grantor = context.accounts.grantor.key();
    authorization.grantee = params.grantee;
    authorization.scope = params.scope;
    authorization.delegates = vec![];
    Ok(())
}
