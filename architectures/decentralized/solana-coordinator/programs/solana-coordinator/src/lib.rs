use anchor_lang::prelude::*;

mod client_id;
pub use client_id::ClientId;
use psyche_coordinator::Coordinator;

declare_id!("2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx");

#[derive(Debug, InitSpace)]
#[account]
pub struct CoordinatorManager {
    pub coordinator: Coordinator<ClientId>,
}

#[program]
pub mod solana_coordinator {
    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinator>,
        run_id: String,
        warmup_time: u64,
        cooldown_time: u64,
    ) -> Result<()> {
        let coordinator = &mut ctx.accounts.coordinator;

        coordinator.coordinator.run_id = run_id;
        coordinator.coordinator.warmup_time = warmup_time;
        coordinator.coordinator.cooldown_time = cooldown_time;

        msg!("Coordinator: {:?}", coordinator);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeCoordinator<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + CoordinatorManager::INIT_SPACE,
    )]
    pub coordinator: Account<'info, CoordinatorManager>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}
