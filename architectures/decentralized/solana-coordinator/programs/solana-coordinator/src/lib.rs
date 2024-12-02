use anchor_lang::prelude::*;

mod client_id;

pub use client_id::ClientId;

declare_id!("93nSTEimZTz6cMN6KCkEjJPbSxMtTBf1kntSRzB9bTsQ");

#[program]
pub mod solana_coordinator {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
