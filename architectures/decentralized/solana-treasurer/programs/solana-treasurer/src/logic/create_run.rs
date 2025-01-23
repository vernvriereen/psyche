use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;

use psyche_solana_coordinator::cpi::accounts::InitializeCoordinatorAccounts;
use psyche_solana_coordinator::cpi::initialize_coordinator;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: CreateRunParams)]
pub struct CreateRunAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Run::space(),
        seeds = [Run::SEED_PREFIX, &params.run_identity],
        bump,
    )]
    pub run: Box<Account<'info, Run>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = collateral_mint,
        associated_token::authority = run,
    )]
    pub run_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub coordinator_instance: UncheckedAccount<'info>,

    #[account(mut)]
    pub coordinator_account: UncheckedAccount<'info>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,

    #[account()]
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct CreateRunParams {
    pub run_identity: [u8; 32],
}

pub fn create_run_processor(
    context: Context<CreateRunAccounts>,
    params: &CreateRunParams,
) -> Result<()> {
    let run_bump = context.bumps.run;
    let run_identity = params.run_identity;

    initialize_coordinator(
        CpiContext::new(
            context.accounts.coordinator_program.to_account_info(),
            InitializeCoordinatorAccounts {
                payer: context.accounts.payer.to_account_info(),
                authority: context.accounts.run.to_account_info(),
                instance: context.accounts.coordinator_instance.to_account_info(),
                account: context.accounts.coordinator_account.to_account_info(),
                system_program: context.accounts.system_program.to_account_info(),
            },
        )
        .with_signer(&[&[Run::SEED_PREFIX, &run_identity, &[run_bump]]]),
        run_identity,
    )?;

    let run = &mut context.accounts.run;
    run.bump = run_bump;
    run.identity = run_identity;
    run.authority = context.accounts.authority.key();
    run.collateral_mint = context.accounts.collateral_mint.key();
    Ok(())
}
