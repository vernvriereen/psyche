use anchor_lang::prelude::*;

declare_id!("8Ff7mrPkP9QD9vER8WVNwf18293HFzawcJG3CjjQKWky");

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
