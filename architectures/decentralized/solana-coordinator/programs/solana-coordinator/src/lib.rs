use anchor_lang::prelude::*;

mod client_id;
pub use client_id::ClientId;
use psyche_coordinator::{Coordinator, MAX_STRING_LEN};

declare_id!("2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx");

#[account]
#[derive(Debug, InitSpace)]
pub struct SolanaCoordinator(Coordinator<ClientId>);

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
        let coordinator = &mut ctx.accounts.coordinator;
        // let coordinator = &mut ctx.accounts.coordinator;

        coordinator.0.run_id = String::from_utf8(run_id.to_vec()).unwrap();
        coordinator.0.warmup_time = warmup_time;
        coordinator.0.cooldown_time = cooldown_time;

        msg!("Coordinator: {:?}", coordinator);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeCoordinator<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + SolanaCoordinator::INIT_SPACE,
    )]
    pub coordinator: Account<'info, SolanaCoordinator>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
