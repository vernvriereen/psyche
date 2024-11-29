use anchor_lang::prelude::*;

mod client_id;

pub use client_id::ClientId;

declare_id!("2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx");

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
