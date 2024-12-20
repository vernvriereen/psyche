use anchor_lang::{prelude::*, system_program};

mod client_id;
pub use client_id::ClientId;
use psyche_coordinator::Coordinator;

declare_id!("5gKtdi6At7WEcLE22GmkSg94rVgc2hRRo3VvKhLnoJZP");

fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..psyche_coordinator::SOLANA_MAX_STRING_LEN.min(str.as_bytes().len())]
}

#[derive(Debug)]
#[account(zero_copy)]
#[repr(C)]
pub struct CoordinatorAccount {
    pub coordinator: Coordinator<ClientId>,
}

#[derive(InitSpace)]
#[account]
pub struct CoordinatorInstance {
    pub bump: u8,
    pub owner: Pubkey,
    pub coordinator: Pubkey,
}

#[program]
pub mod solana_coordinator {
    use std::{cell::RefMut, ops::DerefMut};

    use psyche_coordinator::SOLANA_MAX_STRING_LEN;

    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinator>,
        run_id: String,
    ) -> Result<()> {
        let instance = &mut ctx.accounts.instance;
        instance.bump = ctx.bumps.instance;
        instance.owner = ctx.accounts.payer.key();
        instance.coordinator = ctx.accounts.coordinator.key();

        // this is what AccountLoader::load_init does, but unrolled to deal with weird lifetime stuff
        let mut coordinator: RefMut<CoordinatorAccount> = {
            let acc_info = ctx.accounts.coordinator.as_ref();
            if acc_info.owner != &solana_coordinator::ID {
                return Err(Error::from(ErrorCode::AccountOwnedByWrongProgram)
                    .with_pubkeys((*acc_info.owner, solana_coordinator::ID)));
            }
            if !acc_info.is_writable {
                return Err(ErrorCode::AccountNotMutable.into());
            }
            if !acc_info.is_writable {
                return Err(ErrorCode::AccountNotMutable.into());
            }

            let data = acc_info.try_borrow_mut_data()?;

            // The discriminator should be zero, since we're initializing.
            let disc = CoordinatorAccount::DISCRIMINATOR;
            let given_disc = &data[..disc.len()];
            let has_disc = given_disc.iter().any(|b| *b != 0);
            if has_disc {
                return Err(ErrorCode::AccountDiscriminatorAlreadySet.into());
            }

            RefMut::map(data, |data| {
                bytemuck::from_bytes_mut(
                    &mut data.deref_mut()
                        [disc.len()..std::mem::size_of::<CoordinatorAccount>() + disc.len()],
                )
            })
        };

        let mut array = [0u8; SOLANA_MAX_STRING_LEN];
        let run_id = bytes_from_string(&run_id);
        array[..run_id.len()].copy_from_slice(run_id);
        coordinator.coordinator.run_id = array;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(run_id: String)]
pub struct InitializeCoordinator<'info> {
    #[account(init, payer = payer, space = 8 + CoordinatorInstance::INIT_SPACE, seeds = [b"coordinator", bytes_from_string(&run_id)], bump)]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut)]
    pub coordinator: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}
