mod client_id;

use anchor_lang::{prelude::*, system_program};
use bytemuck::{Pod, Zeroable};
pub use client_id::ClientId;
use psyche_coordinator::{
    model::Model, ClientState, Coordinator, CoordinatorConfig, CoordinatorError, RunState,
    TickResult, Witness, WitnessBloom, WitnessProof, SOLANA_MAX_NUM_CLIENTS, SOLANA_MAX_STRING_LEN,
};
use psyche_core::{sha256v, FixedVec, SizedIterator};
use std::{
    cell::{RefCell, RefMut},
    ops::DerefMut,
    rc::Rc,
};

declare_id!("5gKtdi6At7WEcLE22GmkSg94rVgc2hRRo3VvKhLnoJZP");

#[program]
pub mod psyche_solana_treasurer {
    use super::*;
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

#[error_code]
pub enum ProgramError {
    #[msg("Cannot update config of finished run")]
    UpdateConfigFinished,

    #[msg("Cannot update config when not halted")]
    UpdateConfigNotHalted,

    #[msg("Coordinator account incorrect size")]
    CoordinatorAccountIncorrectSize,

    #[msg("Could not set whitelist")]
    CouldNotSetWhitelist,

    #[msg("Not in whitelist")]
    NotInWhitelist,

    #[msg("Client id mismatch")]
    ClientIdMismatch,

    #[msg("Clients list full")]
    ClientsFull,

    #[msg("Config sanity check failed")]
    ConfigSanityCheckFailed,

    #[msg("Model sanity check failed")]
    ModelSanityCheckFailed,

    #[msg("Signer not a client")]
    SignerNotAClient,

    #[msg("Coordinator error: No active round")]
    CoordinatorErrorNoActiveRound,

    #[msg("Coordinator error: Invalid witness")]
    CoordinatorErrorInvalidWitness,

    #[msg("Coordinator error: Invalid run state")]
    CoordinatorErrorInvalidRunState,

    #[msg("Coordinator error: Duplicate witness")]
    CoordinatorErrorDuplicateWitness,

    #[msg("Coordinator error: Invalid health check")]
    CoordinatorErrorInvalidHealthCheck,

    #[msg("Coordinator error: Halted")]
    CoordinatorErrorHalted,

    #[msg("Coordinator error: Invalid checkpoint")]
    CoordinatorErrorInvalidCheckpoint,

    #[msg("Coordinator error: Witnesses full")]
    CoordinatorErrorWitnessesFull,

    #[msg("Coordinator error: Cannot resume")]
    CoordinatorErrorCannotResume,

    #[msg("Coordinator error: Invalid withdraw")]
    CoordinatorErrorInvalidWithdraw,

    #[msg("Coordinator error: Invalid committee selection")]
    CoordinatorErrorInvalidCommitteeSelection,
}

impl From<CoordinatorError> for ProgramError {
    fn from(value: CoordinatorError) -> Self {
        match value {
            CoordinatorError::NoActiveRound => ProgramError::CoordinatorErrorNoActiveRound,
            CoordinatorError::InvalidWitness => ProgramError::CoordinatorErrorInvalidWitness,
            CoordinatorError::InvalidRunState => ProgramError::CoordinatorErrorInvalidRunState,
            CoordinatorError::DuplicateWitness => ProgramError::CoordinatorErrorDuplicateWitness,
            CoordinatorError::InvalidHealthCheck => {
                ProgramError::CoordinatorErrorInvalidHealthCheck
            }
            CoordinatorError::Halted => ProgramError::CoordinatorErrorNoActiveRound,
            CoordinatorError::InvalidCheckpoint => ProgramError::CoordinatorErrorInvalidCheckpoint,
            CoordinatorError::WitnessesFull => ProgramError::CoordinatorErrorWitnessesFull,
            CoordinatorError::CannotResume => ProgramError::CoordinatorErrorCannotResume,
            CoordinatorError::InvalidWithdraw => ProgramError::CoordinatorErrorInvalidWithdraw,
            CoordinatorError::InvalidCommitteeSelection => {
                ProgramError::CoordinatorErrorInvalidCommitteeSelection
            }
        }
    }
}
