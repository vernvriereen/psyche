use std::ops::Deref;

use anchor_lang::prelude::*;

mod client_id;

pub use client_id::ClientId;
use psyche_coordinator::{Coordinator, MAX_STRING_LEN};
use psyche_core::NodeIdentity;

declare_id!("2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx");

#[program]
pub mod solana_coordinator {
    use psyche_coordinator::Coordinator;

    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinator>,
        run_id: [u8; MAX_STRING_LEN],
        warmup_time: u64,
        cooldown_time: u64,
    ) -> Result<()> {
        let mut coordinator = &mut ctx.accounts.coordinator;

        *coordinator = Coordinator::default();
        coordinator.run_id = run_id;
        coordinator.warmup_time = warmup_time;
        coordinator.cooldown_time = cooldown_time;

        msg!("Coordinator: {:?}", coordinator);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeCoordinator<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + Coordinator::<ClientId>::INIT_SPACE // Add space calculation below
    )]
    pub coordinator: Account<'info, Coordinator<ClientId>>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
