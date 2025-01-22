use anchor_lang::prelude::*;

use psyche_solana_coordinator::cpi::accounts::InitializeCoordinatorAccounts;
use psyche_solana_coordinator::cpi::initialize_coordinator;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

#[derive(Accounts)]
#[instruction(run_identity: [u8; 32])]
pub struct CreateRunAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Run::space(),
        seeds = [Run::SEED_PREFIX, params.run_identity],
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
    pub system_program: Program<'info, System>,
}

pub fn create_run_processor(ctx: Context<CreateRunAccounts>, run_identity: [u8; 32]) -> Result<()> {
    let cpi_context = CpiContext::new(
        ctx.accounts.coordinator_program.to_account_info(),
        InitializeCoordinatorAccounts {
            payer: ctx.accounts.payer.to_account_info(),
            instance: ctx.accounts.coordinator_instance.to_account_info(),
            account: ctx.accounts.coordinator_account.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        },
    );
    let run_id = String::from_utf8(run_identity);
    initialize_coordinator(cpi_context, run_id)?;

    let run = &mut context.accounts.run;
    run.bump = context.bumps.run;
    run.identity = params.run_identity;
    run.authority = context.accounts.authority.key();
    run.collateral_mint = context.accounts.collateral_mint.key();

    Ok(())
}
