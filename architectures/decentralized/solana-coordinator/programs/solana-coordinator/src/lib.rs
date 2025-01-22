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

declare_id!("5gKtdi6At7WEcLE22GmkSg94rVgc2hRRo3VvKhLnoJZP");

pub const SOLANA_MAX_NUM_PENDING_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;
pub const SOLANA_MAX_NUM_WHITELISTED_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;

pub fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..SOLANA_MAX_STRING_LEN.min(str.len())]
}

pub fn coordinator_account_from_bytes(
    bytes: &[u8],
) -> std::result::Result<&CoordinatorAccount, ProgramError> {
    if bytes.len() != CoordinatorAccount::size_with_discriminator() {
        return Err(ProgramError::CoordinatorAccountIncorrectSize);
    }
    if &bytes[..CoordinatorAccount::DISCRIMINATOR.len()] != CoordinatorAccount::DISCRIMINATOR {
        return Err(ProgramError::CoordinatorAccountInvalidDiscriminator);
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
    pub authority: Pubkey,
    pub account: Pubkey,
    #[max_len(SOLANA_MAX_STRING_LEN)]
    pub run_id: String,
}

#[program]
pub mod psyche_solana_coordinator {
    use super::*;

    pub fn initialize_coordinator(
        ctx: Context<InitializeCoordinatorAccounts>,
        run_id: String,
    ) -> Result<()> {
        // Initialize the coordinator instance
        let instance = &mut ctx.accounts.instance;
        instance.bump = ctx.bumps.instance;
        instance.owner = ctx.accounts.authority.key();
        instance.account = ctx.accounts.account.key();
        instance.run_id = run_id.clone();
        // Initialize the coordinator account
        let mut data = ctx.accounts.account.try_borrow_mut_data()?;
        if data.len() != CoordinatorAccount::size_with_discriminator() {
            return err!(ProgramError::CoordinatorAccountIncorrectSize);
        }
        // Install the correct coordinator account's discriminator, verify that it was zero before init
        let disc = CoordinatorAccount::DISCRIMINATOR;
        let data_disc = &mut data[..disc.len()];
        if data_disc.iter().any(|b| *b != 0) {
            return Err(ErrorCode::AccountDiscriminatorAlreadySet.into());
        }
        data_disc.copy_from_slice(disc);
        // Ready to prepare the coordinator content
        let account = bytemuck::from_bytes_mut::<CoordinatorAccount>(
            &mut data[disc.len()..CoordinatorAccount::size_with_discriminator()],
        );
        // Setup the run_id const
        let mut array = [0u8; SOLANA_MAX_STRING_LEN];
        let run_id = bytes_from_string(&run_id);
        array[..run_id.len()].copy_from_slice(run_id);
        account.state.coordinator.run_id = array;
        // Done
        Ok(())
    }

    pub fn free_coordinator(ctx: Context<FreeCoordinatorAccounts>) -> Result<()> {
        if !&ctx.accounts.account.load()?.state.coordinator.halted() {
            return err!(ProgramError::CloseCoordinatorNotHalted);
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
    #[account(
        init,
        payer = payer,
        space = 8 + CoordinatorInstance::INIT_SPACE,
        seeds = [b"coordinator", bytes_from_string(&run_id)],
        bump
    )]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(mut)]
    pub account: UncheckedAccount<'info>,
    #[account()]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct OwnerCoordinatorAccounts<'info> {
    #[account(
        seeds = [b"coordinator", bytes_from_string(&instance.run_id)],
        bump = instance.bump,
        constraint = instance.owner == *authority.key
    )]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(
        mut,
        owner = crate::ID,
        constraint = instance.account == account.key()
    )]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PermissionlessCoordinatorAccounts<'info> {
    #[account(
        seeds = [b"coordinator", bytes_from_string(&instance.run_id)],
        bump = instance.bump
    )]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(
        mut,
        owner = crate::ID,
        constraint = instance.account == account.key()
    )]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FreeCoordinatorAccounts<'info> {
    #[account(
        mut,
        seeds = [b"coordinator", bytes_from_string(&instance.run_id)],
        bump = instance.bump,
        constraint = instance.owner == *owner.key,
        close = reimbursement
    )]
    pub instance: Account<'info, CoordinatorInstance>,
    #[account(
        mut,
        owner = crate::ID,
        constraint = instance.account == account.key()
    )]
    pub account: AccountLoader<'info, CoordinatorAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub reimbursement: UncheckedAccount<'info>,
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}
