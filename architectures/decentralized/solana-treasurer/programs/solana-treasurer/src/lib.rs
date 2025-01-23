pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
pub use logic::*;

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn create_run(ctx: Context<CreateRunAccounts>, params: CreateRunParams) -> Result<()> {
        create_run_processor(ctx, &params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("Invalid parameter")]
    InvalidParameter,
}
