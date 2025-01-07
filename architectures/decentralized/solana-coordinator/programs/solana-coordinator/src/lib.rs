mod client_id;

use anchor_lang::{prelude::*, system_program};
use bytemuck::{Pod, Zeroable};
pub use client_id::ClientId;
use psyche_coordinator::{
    ClientState, CoodinatorConfig, Coordinator, CoordinatorError, RunState, TickResult,
    SOLANA_MAX_NUM_CLIENTS, SOLANA_MAX_STRING_LEN,
};
use psyche_core::{sha256v, FixedVec, SizedIterator};
use std::{
    cell::{RefCell, RefMut},
    ops::DerefMut,
    rc::Rc,
};

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

#[derive(Debug, Clone, Copy, Default, Zeroable, InitSpace, Pod)]
#[repr(C)]
pub struct Client {
    owner: Pubkey,
    id: ClientId,
    staked: u64,
    earned: u64,
    slashed: u64,
    active: u64,
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

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub struct CoordinatorInstanceState {
    pub coordinator: Coordinator<ClientId>,
    pub clients_state: ClientsState,
}

unsafe impl Pod for CoordinatorInstanceState {}

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub struct ClientsState {
    pub whitelist: FixedVec<ClientId, SOLANA_MAX_NUM_WHITELISTED_CLIENTS>,
    pub clients: FixedVec<Client, SOLANA_MAX_NUM_PENDING_CLIENTS>,
    pub next_active: u64,
}

unsafe impl Pod for ClientsState {}

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

    pub fn update_coordinator_config(
        ctx: Context<OwnerCoordinatorAccounts>,
        config: CoodinatorConfig<ClientId>,
    ) -> Result<()> {
        let coordinator = &mut ctx.accounts.account.load_mut()?.state.coordinator;

        if coordinator.run_state == RunState::Finished {
            return err!(ProgramError::UpdateConfigFinished);
        } else if !coordinator.halted() {
            return err!(ProgramError::UpdateConfigNotHalted);
        }

        // TODO: add sanity checks

        let _ = std::mem::replace(&mut coordinator.config, config);

        if coordinator.run_state == RunState::Uninitialized {
            // this is the only way to get out of uninitialized
            // basically we're requiring a call to update_coordinator_config before
            // we can start

            coordinator.run_state = RunState::Paused;
            // resume() copies the previous epoch's progress
            // step 1 is the first valid step
            coordinator.prev_epoch_progress.step = 1;
        }

        Ok(())
    }

    pub fn set_whitelist(
        ctx: Context<OwnerCoordinatorAccounts>,
        clients: Vec<ClientId>,
    ) -> Result<()> {
        let clients_state = &mut ctx.accounts.account.load_mut()?.state.clients_state;
        clients_state.whitelist.clear();
        clients_state
            .whitelist
            .extend(clients.into_iter())
            .map_err(|_| ProgramError::CouldNotSetWhitelist)?;
        Ok(())
    }

    pub fn join_run(ctx: Context<PermissionlessCoordinatorAccounts>, id: ClientId) -> Result<()> {
        let clients_state = &mut ctx.accounts.account.load_mut()?.state.clients_state;

        let owner = *ctx.accounts.payer.signer_key().unwrap();

        if !clients_state.whitelist.is_empty() {
            if clients_state
                .whitelist
                .iter()
                .find(|x| x.owner == owner)
                .is_none()
            {
                return err!(ProgramError::NotInWhitelist);
            }
        } else {
            panic!("no whitelist");
        }

        let exisiting = match clients_state.clients.iter_mut().find(|x| x.owner == owner) {
            Some(client) => {
                if client.id != id {
                    return err!(ProgramError::ClientIdMismatch);
                }
                client.active = clients_state.next_active;
                true
            }
            None => false,
        };

        if !exisiting {
            if clients_state
                .clients
                .push(Client {
                    owner,
                    id,
                    staked: 0,
                    earned: 0,
                    slashed: 0,
                    active: clients_state.next_active,
                })
                .is_err()
            {
                return err!(ProgramError::ClientsFull);
            }
        }

        Ok(())
    }

    pub fn set_paused(ctx: Context<OwnerCoordinatorAccounts>, paused: bool) -> Result<()> {
        let coordinator = &mut ctx.accounts.account.load_mut()?.state.coordinator;

        if let Err(err) = match paused {
            true => coordinator.pause(),
            false => coordinator.resume(Clock::get()?.unix_timestamp as u64),
        } {
            return err!(ProgramError::from(err));
        }
        Ok(())
    }

    pub fn tick(ctx: Context<PermissionlessCoordinatorAccounts>) -> Result<()> {
        let state = &mut ctx.accounts.account.load_mut()?.state;

        let clock: Clock = Clock::get()?;
        let random_seed_bytes = sha256v(&[
            &clock.unix_timestamp.to_ne_bytes(),
            &clock.slot.to_ne_bytes(),
        ]);

        let mut random_seed: [u8; 8] = [0; 8];
        random_seed.copy_from_slice(&random_seed_bytes[..8]);

        let active_clients = match state.coordinator.run_state {
            RunState::WaitingForMembers => Some(state.clients_state.active_clients()),
            _ => None,
        };

        match state.coordinator.tick(
            active_clients,
            clock.unix_timestamp as u64,
            u64::from_ne_bytes(random_seed),
        ) {
            Ok(TickResult::Ticked) => Ok(()),
            Ok(TickResult::EpochEnd(_)) => {
                state.clients_state.next_active += 1;

                let mut i = 0;
                let mut j = 0;
                let finished_clients = &state.coordinator.epoch_state.clients;
                let exited_clients = &state.coordinator.epoch_state.exited_clients;

                for client in state.clients_state.clients.iter_mut() {
                    if i < finished_clients.len() {
                        if client.id == finished_clients[i].id {
                            if finished_clients[i].state == ClientState::Healthy {
                                client.earned += 1;
                            }
                            i += 1;
                        }
                    }

                    if j < exited_clients.len() {
                        if client.id == exited_clients[j].id {
                            if exited_clients[j].state == ClientState::Ejected {
                                client.slashed += 1;
                            }
                            j += 1;
                        }
                    }
                }

                Ok(())
            }
            Err(err) => {
                err!(ProgramError::from(err))
            }
        }
    }
}

impl ClientsState {
    fn active_clients(&self) -> SizedIterator<impl Iterator<Item = &ClientId>> {
        let size = Rc::new(RefCell::new(0));
        let size_clone = size.clone();

        let iter = self
            .clients
            .iter()
            .filter_map(move |x| match x.active == self.next_active {
                true => {
                    *size_clone.borrow_mut() += 1;
                    Some(&x.id)
                }
                false => None,
            });

        let size = *size.borrow();
        SizedIterator::new(iter, size)
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
        }
    }
}
