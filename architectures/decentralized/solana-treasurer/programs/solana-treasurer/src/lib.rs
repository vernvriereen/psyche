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

pub fn run_identity_from_string(string: &str) -> Pubkey {
    let mut bytes = vec![];
    bytes.extend_from_slice(string.as_bytes());
    bytes.resize(32, 0);
    Pubkey::new_from_array(bytes.try_into().unwrap())
}
