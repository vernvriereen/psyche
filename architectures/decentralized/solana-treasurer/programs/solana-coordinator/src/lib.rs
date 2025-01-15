use anchor_lang::{prelude::*, system_program};

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinatorAccounts>,
        run_id: String,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(run_id: String)]
pub struct InitializeCoordinatorAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum ProgramError {
    Overflow,
}
