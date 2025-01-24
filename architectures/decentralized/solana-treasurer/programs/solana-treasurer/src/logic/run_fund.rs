use anchor_lang::prelude::*;
use anchor_spl::token::transfer;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;

use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: RunFundParams)]
pub struct RunFundAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = authority_collateral.mint == run.collateral_mint,
        constraint = authority_collateral.owner == authority.key(),
        constraint = authority_collateral.delegate == None.into(),
    )]
    pub authority_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = run.authority == authority.key(),
    )]
    pub run: Box<Account<'info, Run>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = run,
    )]
    pub run_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RunFundParams {
    pub collateral_amount: u64,
}

pub fn run_fund_processor(context: Context<RunFundAccounts>, params: RunFundParams) -> Result<()> {
    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                from: context.accounts.authority_collateral.to_account_info(),
                to: context.accounts.run_collateral.to_account_info(),
                authority: context.accounts.authority.to_account_info(),
            },
        ),
        params.collateral_amount,
    )?;

    let run = &mut context.accounts.run;
    run.total_funded_collateral_amount += params.collateral_amount;

    Ok(())
}
