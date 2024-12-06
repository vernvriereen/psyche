use anchor_lang::prelude::*;

mod client_id;
pub use client_id::ClientId;
use psyche_coordinator::Coordinator;

declare_id!("2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx");

#[derive(Debug, InitSpace)]
#[account(zero_copy)]
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
        let coordinator = &mut ctx.accounts.coordinator.load_init()?;

        let mut array = [0u8; 64]; 
        let bytes = run_id.as_bytes(); 

        let len = 64.min(bytes.len());
        array[..len].copy_from_slice(&bytes[..len]);

        coordinator.coordinator.run_id = array;
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
        space = 10 * (1024 as usize)
    )]
    pub coordinator: AccountLoader<'info, CoordinatorManager>,
    #[account(mut)]
    pub signer: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}
