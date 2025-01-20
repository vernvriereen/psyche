use anchor_lang::prelude::*;

use psyche_solana_coordinator::cpi::accounts::InitializeCoordinatorAccounts;
use psyche_solana_coordinator::cpi::initialize_coordinator;
use psyche_solana_coordinator::program::PsycheSolanaCoordinator;

#[derive(Accounts)]
#[instruction(run_id: String)]
pub struct CreateRunAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(mut)]
    pub coordinator_instance: UncheckedAccount<'info>,

    #[account(mut)]
    pub coordinator_account: UncheckedAccount<'info>,

    #[account()]
    pub coordinator_program: Program<'info, PsycheSolanaCoordinator>,

    #[account()]
    pub system_program: Program<'info, System>,
}

pub fn create_run_logic(ctx: Context<CreateRunAccounts>, run_id: String) -> Result<()> {
    let cpi_context = CpiContext::new(
        ctx.accounts.coordinator_program.to_account_info(),
        InitializeCoordinatorAccounts {
            payer: ctx.accounts.payer.to_account_info(),
            instance: ctx.accounts.coordinator_instance.to_account_info(),
            account: ctx.accounts.coordinator_account.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        },
    );
    initialize_coordinator(cpi_context, run_id)?;
    Ok(())
}
