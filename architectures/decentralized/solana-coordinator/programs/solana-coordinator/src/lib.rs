mod client;
mod clients_state;
mod instance_state;
mod program_error;

use anchor_lang::{prelude::*, system_program};
pub use client::ClientId;
pub use instance_state::CoordinatorInstanceState;
pub use program_error::ProgramError;
use psyche_coordinator::{
    model::Model, Committee, CommitteeProof, CoordinatorConfig, Witness, WitnessBloom,
    WitnessProof, SOLANA_MAX_NUM_CLIENTS, SOLANA_MAX_STRING_LEN,
};
use std::{cell::RefMut, ops::DerefMut};

declare_id!("5gKtdi6At7WEcLE22GmkSg94rVgc2hRRo3VvKhLnoJZP");

pub const SOLANA_MAX_NUM_PENDING_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;
pub const SOLANA_MAX_NUM_WHITELISTED_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;

pub fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..psyche_coordinator::SOLANA_MAX_STRING_LEN.min(str.as_bytes().len())]
}

pub fn coordinator_account_from_bytes(
    bytes: &[u8],
) -> std::result::Result<&CoordinatorAccount, ProgramError> {
    if bytes.len() != CoordinatorAccount::size_with_discriminator() {
        return Err(ProgramError::CoordinatorAccountIncorrectSize);
    }
    Ok(bytemuck::from_bytes(
        &bytes[CoordinatorAccount::DISCRIMINATOR.len()
            ..CoordinatorAccount::size_with_discriminator()],
    ))
}

#[account(zero_copy)]
#[repr(C)]
pub struct CoordinatorAccount {
    pub state: CoordinatorInstanceState,
}
impl CoordinatorAccount {
    pub fn size_with_discriminator() -> usize {
        CoordinatorAccount::DISCRIMINATOR.len() + std::mem::size_of::<CoordinatorAccount>()
    }
}

#[derive(Debug, InitSpace)]
#[account]
pub struct CoordinatorInstance {
    pub bump: u8,
    pub owner: Pubkey,
    pub account: Pubkey,
    #[max_len(SOLANA_MAX_STRING_LEN)]
    pub run_id: String,
}

#[program]
pub mod solana_coordinator {
    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinatorAccounts>,
        run_id: String,
    ) -> Result<()> {
        let instance = &mut ctx.accounts.instance;
        instance.bump = ctx.bumps.instance;
        instance.owner = ctx.accounts.payer.key();
        instance.account = ctx.accounts.account.key();
        instance.run_id = run_id.clone();

        // this is what AccountLoader::load_init does, but unrolled to deal with weird lifetime stuff
        let mut account: RefMut<CoordinatorAccount> = {
            let acc_info = ctx.accounts.account.as_ref();
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

            let mut data = acc_info.try_borrow_mut_data()?;

            // The discriminator should be zero, since we're initializing.
            let disc = CoordinatorAccount::DISCRIMINATOR;
            let given_disc = &data[..disc.len()];
            let has_disc = given_disc.iter().any(|b| *b != 0);
            if has_disc {
                return Err(ErrorCode::AccountDiscriminatorAlreadySet.into());
            }

            if data.len() != CoordinatorAccount::size_with_discriminator() {
                return err!(ProgramError::CoordinatorAccountIncorrectSize);
            }

            {
                data.deref_mut()[..disc.len()].copy_from_slice(disc);
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
        account.state.coordinator.run_id = array;

        Ok(())
    }

    pub fn free_coordinator(ctx: Context<FreeCoordinatorAccounts>) -> Result<()> {
        {
            let state = &ctx.accounts.account.load()?.state;
            if !state.coordinator.halted() {
                return err!(ProgramError::CloseCoordinatorNotHalted);
            }
        }
        ctx.accounts
            .account
            .close(ctx.accounts.payer.to_account_info())
    }

    pub fn update_coordinator_config_model(
        ctx: Context<OwnerCoordinatorAccounts>,
        config: Option<CoordinatorConfig<ClientId>>,
        model: Option<Model>,
    ) -> Result<()> {
        ctx.accounts
            .account
            .load_mut()?
            .state
            .update_coordinator_config_model(config, model)
    }

    pub fn set_whitelist(
        ctx: Context<OwnerCoordinatorAccounts>,
        clients: Vec<Pubkey>,
    ) -> Result<()> {
        ctx.accounts
            .account
            .load_mut()?
            .state
            .set_whitelist(clients)
    }

    pub fn join_run(ctx: Context<PermissionlessCoordinatorAccounts>, id: ClientId) -> Result<()> {
        if &id.signer != ctx.accounts.payer.key {
            return err!(ProgramError::SignerMismatch);
        }
        ctx.accounts.account.load_mut()?.state.join_run(id)
    }

    pub fn set_paused(ctx: Context<OwnerCoordinatorAccounts>, paused: bool) -> Result<()> {
        ctx.accounts.account.load_mut()?.state.set_paused(paused)
    }

    pub fn tick(ctx: Context<PermissionlessCoordinatorAccounts>) -> Result<()> {
        ctx.accounts.account.load_mut()?.state.tick()
    }

    pub fn witness(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        proof: WitnessProof,
        participant_bloom: WitnessBloom,
        order_bloom: WitnessBloom,
    ) -> Result<()> {
        ctx.accounts.account.load_mut()?.state.witness(
            ctx.accounts.payer.key,
            Witness {
                proof,
                participant_bloom,
                order_bloom,
            },
        )
    }

    pub fn health_check(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        committee: Committee,
        position: u64,
        index: u64,
    ) -> Result<()> {
        ctx.accounts.account.load_mut()?.state.health_check(
            ctx.accounts.payer.key,
            vec![CommitteeProof {
                committee,
                position,
                index,
            }],
        )
    }
}

#[derive(Accounts)]
#[instruction(run_id: String)]
pub struct InitializeCoordinatorAccounts<'info> {
    #[account(init, payer = payer, space = 8 + CoordinatorInstance::INIT_SPACE, seeds = [b"coordinator", bytes_from_string(&run_id)], bump)]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut)]
    pub account: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct OwnerCoordinatorAccounts<'info> {
    #[account(seeds = [b"coordinator", bytes_from_string(&instance.run_id)], bump = instance.bump, constraint = instance.owner == *payer.key)]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut, owner = crate::ID, constraint = instance.account == account.key())]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PermissionlessCoordinatorAccounts<'info> {
    #[account(seeds = [b"coordinator", bytes_from_string(&instance.run_id)], bump = instance.bump)]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut, owner = crate::ID, constraint = instance.account == account.key())]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FreeCoordinatorAccounts<'info> {
    #[account(mut, seeds = [b"coordinator", bytes_from_string(&instance.run_id)], bump = instance.bump, constraint = instance.owner == *payer.key, close = payer)]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut, owner = crate::ID, constraint = instance.account == account.key())]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}
