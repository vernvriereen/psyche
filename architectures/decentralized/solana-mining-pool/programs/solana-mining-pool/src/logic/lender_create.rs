use anchor_lang::prelude::*;

use crate::state::Lender;
use crate::state::Pool;

#[derive(Accounts)]
#[instruction(params: LenderCreateParams)]
pub struct LenderCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub user: Signer<'info>,

    #[account()]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = payer,
        space = Lender::space_with_discriminator(),
        seeds = [
            Lender::SEEDS_PREFIX,
            pool.key().as_ref(),
            user.key().as_ref()
        ],
        bump
    )]
    pub lender: Box<Account<'info, Lender>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct LenderCreateParams {}

pub fn lender_create_processor(
    context: Context<LenderCreateAccounts>,
    _params: LenderCreateParams,
) -> Result<()> {
    let lender = &mut context.accounts.lender;

    lender.bump = context.bumps.lender;

    lender.deposited_collateral_amount = 0;
    lender.claimed_redeemable_amount = 0;

    Ok(())
}
