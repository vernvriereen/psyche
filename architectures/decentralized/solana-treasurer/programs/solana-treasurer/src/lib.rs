pub mod error;
pub mod logic;
pub mod state;

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

use anchor_lang::prelude::*;
use logic::*;

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn create_run(ctx: Context<CreateRunAccounts>, run_id: String) -> Result<()> {
        create_run_processor(ctx, run_id)
    }
}

#[error_code]
pub enum ProgramError {
    Overflow,
}
