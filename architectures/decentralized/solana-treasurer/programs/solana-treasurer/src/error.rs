use anchor_lang::prelude::*;

#[error_code]
pub enum ProgramError {
    #[msg("Invalid parameter")]
    InvalidParameter,
}
