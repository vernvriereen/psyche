pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
pub use logic::*;

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn run_create(ctx: Context<RunCreateAccounts>, params: RunCreateParams) -> Result<()> {
        run_create_processor(ctx, &params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("Invalid parameter")]
    InvalidParameter,
}
