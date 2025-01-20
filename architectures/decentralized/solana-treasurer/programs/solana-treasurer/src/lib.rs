use anchor_lang::prelude::*;

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

pub mod create_run;

use create_run::*;

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn create_run(ctx: Context<CreateRunAccounts>, run_id: String) -> Result<()> {
        create_run_logic(ctx, run_id)
    }
}

#[error_code]
pub enum ProgramError {
    Overflow,
}
