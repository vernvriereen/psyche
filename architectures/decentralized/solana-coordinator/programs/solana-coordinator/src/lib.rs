mod client;
mod clients_state;
mod instance_state;
pub mod logic;
mod program_error;

use anchor_lang::prelude::*;
pub use client::Client;
pub use client::ClientId;
pub use instance_state::CoordinatorInstanceState;
use logic::*;
pub use program_error::ProgramError;
use psyche_coordinator::model::Model;
use psyche_coordinator::Committee;
use psyche_coordinator::CommitteeProof;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::Witness;
use psyche_coordinator::WitnessBloom;
use psyche_coordinator::WitnessProof;
use psyche_coordinator::SOLANA_MAX_NUM_CLIENTS;
use psyche_coordinator::SOLANA_MAX_STRING_LEN;
use psyche_core::MerkleRoot;

declare_id!("3RL7dHgZnuDCqT1FuKg9doJV6W7JYAxCGf2Tgq4rLfU3");

pub const SOLANA_MAX_NUM_PENDING_CLIENTS: usize = SOLANA_MAX_NUM_CLIENTS;

pub fn bytes_from_string(str: &str) -> &[u8] {
    &str.as_bytes()[..SOLANA_MAX_STRING_LEN.min(str.len())]
}

pub fn find_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[CoordinatorInstance::SEEDS_PREFIX, bytes_from_string(run_id)],
        &crate::ID,
    )
    .0
}

pub fn coordinator_account_from_bytes(
    bytes: &[u8],
) -> std::result::Result<&CoordinatorAccount, ProgramError> {
    if bytes.len() != CoordinatorAccount::space_with_discriminator() {
        return Err(ProgramError::CoordinatorAccountIncorrectSize);
    }
    if &bytes[..CoordinatorAccount::DISCRIMINATOR.len()]
        != CoordinatorAccount::DISCRIMINATOR
    {
        return Err(ProgramError::CoordinatorAccountInvalidDiscriminator);
    }
    Ok(bytemuck::from_bytes(
        &bytes[CoordinatorAccount::DISCRIMINATOR.len()
            ..CoordinatorAccount::space_with_discriminator()],
    ))
}

#[account(zero_copy)]
#[repr(C)]
pub struct CoordinatorAccount {
    pub state: CoordinatorInstanceState,
}
impl CoordinatorAccount {
    pub fn space_with_discriminator() -> usize {
        CoordinatorAccount::DISCRIMINATOR.len()
            + std::mem::size_of::<CoordinatorAccount>()
    }
}

#[derive(Debug, InitSpace)]
#[account]
pub struct CoordinatorInstance {
    pub bump: u8,
    pub main_authority: Pubkey,
    pub join_authority: Pubkey,
    pub coordinator_account: Pubkey,
    #[max_len(SOLANA_MAX_STRING_LEN)]
    pub run_id: String,
}

impl CoordinatorInstance {
    pub const SEEDS_PREFIX: &'static [u8] = b"coordinator";
}

#[program]
pub mod psyche_solana_coordinator {
    use psyche_core::MerkleRoot;

    use super::*;

    pub fn init_coordinator(
        context: Context<InitCoordinatorAccounts>,
        params: InitCoordinatorParams,
    ) -> Result<()> {
        init_coordinator_processor(context, params)
    }

    pub fn free_coordinator(
        context: Context<FreeCoordinatorAccounts>,
        params: FreeCoordinatorParams,
    ) -> Result<()> {
        free_coordinator_processor(context, params)
    }

    pub fn update_coordinator_config_model(
        ctx: Context<OwnerCoordinatorAccounts>,
        config: Option<CoordinatorConfig<ClientId>>,
        model: Option<Model>,
    ) -> Result<()> {
        ctx.accounts
            .coordinator_account
            .load_mut()?
            .state
            .update_coordinator_config_model(config, model)
    }

    pub fn set_future_epoch_rates(
        ctx: Context<OwnerCoordinatorAccounts>,
        epoch_earning_rate: Option<u64>,
        epoch_slashing_rate: Option<u64>,
    ) -> Result<()> {
        ctx.accounts
            .coordinator_account
            .load_mut()?
            .state
            .set_future_epoch_rates(epoch_earning_rate, epoch_slashing_rate)
    }

    pub fn join_run(
        context: Context<JoinRunAccounts>,
        params: JoinRunParams,
    ) -> Result<()> {
        join_run_processor(context, params)
    }

    pub fn set_paused(
        ctx: Context<OwnerCoordinatorAccounts>,
        paused: bool,
    ) -> Result<()> {
        ctx.accounts
            .coordinator_account
            .load_mut()?
            .state
            .set_paused(paused)
    }

    pub fn tick(ctx: Context<PermissionlessCoordinatorAccounts>) -> Result<()> {
        ctx.accounts.coordinator_account.load_mut()?.state.tick()
    }

    pub fn witness(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        proof: WitnessProof,
        participant_bloom: WitnessBloom,
        broadcast_bloom: WitnessBloom,
        broadcast_merkle: MerkleRoot,
    ) -> Result<()> {
        ctx.accounts.coordinator_account.load_mut()?.state.witness(
            ctx.accounts.user.key,
            Witness {
                proof,
                participant_bloom,
                broadcast_bloom,
                broadcast_merkle,
            },
        )
    }

    pub fn health_check(
        ctx: Context<PermissionlessCoordinatorAccounts>,
        id: ClientId,
        committee: Committee,
        position: u64,
        index: u64,
    ) -> Result<()> {
        ctx.accounts
            .coordinator_account
            .load_mut()?
            .state
            .health_check(
                ctx.accounts.user.key,
                vec![(
                    id,
                    CommitteeProof {
                        committee,
                        position,
                        index,
                    },
                )],
            )
    }
}

#[derive(Accounts)]
pub struct OwnerCoordinatorAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump,
        constraint = coordinator_instance.main_authority == authority.key()
    )]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key()
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}

#[derive(Accounts)]
pub struct PermissionlessCoordinatorAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump
    )]
    pub coordinator_instance: Account<'info, CoordinatorInstance>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key()
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}
