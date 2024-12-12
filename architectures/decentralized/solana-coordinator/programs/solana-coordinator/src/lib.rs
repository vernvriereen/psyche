use anchor_lang::{prelude::*, system_program};

mod client_id;
pub use client_id::ClientId;
use psyche_coordinator::Coordinator;

declare_id!("5RfSkScUH2mTdiBGAAVmfBVSXpVYs2r4GnWsQjDyZrG7");

#[derive(Debug, InitSpace)]
#[account(zero_copy)]
#[repr(C)]
pub struct CoordinatorManager {
    pub coordinator: Coordinator<ClientId>,
}

#[program]
pub mod solana_coordinator {
    use psyche_coordinator::SOLANA_MAX_STRING_LEN;

    use super::*;

    pub fn initialize_coordinator(_ctx: Context<InitializeCoordinator>) -> Result<()> {
        msg!("Initialized!");
        Ok(())
    }

    pub fn set_run_id(ctx: Context<SetRunID>, run_id: String) -> Result<()> {
        let coordinator = &mut ctx.accounts.coordinator.load_mut()?;
        let mut array = [0u8; SOLANA_MAX_STRING_LEN];
        let bytes = run_id.as_bytes();

        let len = SOLANA_MAX_STRING_LEN.min(bytes.len());
        array[..len].copy_from_slice(&bytes[..len]);

        coordinator.coordinator.run_id = array;
        let new_run_id = String::from_utf8(coordinator.coordinator.run_id.to_vec()).unwrap();
        msg!("New run ID: {}", new_run_id);
        Ok(())
    }

    pub fn increase_coordinator(_ctx: Context<IncreaseCoordinator>, len: u16) -> Result<()> {
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
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetRunID<'info> {
    #[account(mut)]
    pub coordinator: AccountLoader<'info, CoordinatorManager>,
}

#[derive(Accounts)]
#[instruction(len: u16)]
pub struct IncreaseCoordinator<'info> {
    #[account(mut,
        realloc = len as usize,
        realloc::zero = true,
        realloc::payer=signer)]
    pub coordinator: AccountLoader<'info, CoordinatorManager>,
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}
