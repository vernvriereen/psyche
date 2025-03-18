use crate::client::Client;
use crate::clients_state::ClientsState;
use crate::ClientId;
use crate::ProgramError;
use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use psyche_coordinator::{
    model::Model, ClientState, Coordinator, CoordinatorConfig, HealthChecks,
    RunState, TickResult, Witness, SOLANA_MAX_STRING_LEN,
};
use psyche_core::sha256v;
use psyche_core::{FixedString, SizedIterator, SmallBoolean};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
    Default,
)]
#[repr(C)]
pub struct RunMetadata {
    pub name: FixedString<{ SOLANA_MAX_STRING_LEN }>,

    pub description: FixedString<280>,

    pub num_parameters: u64,
    pub vocab_size: u64,
}

#[derive(
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[repr(C)]
pub struct CoordinatorInstanceState {
    pub metadata: RunMetadata,
    pub coordinator: Coordinator<ClientId>,
    pub clients_state: ClientsState,
    pub is_warmup_first_tick: SmallBoolean,
    pub is_training_first_tick: SmallBoolean,
}

unsafe impl Pod for CoordinatorInstanceState {}

impl CoordinatorInstanceState {
    fn get_random_seed(clock: &Clock) -> u64 {
        let random_seed_bytes = sha256v(&[
            &clock.unix_timestamp.to_ne_bytes(),
            &clock.slot.to_ne_bytes(),
        ]);

        let mut random_seed: [u8; 8] = [0; 8];
        random_seed.copy_from_slice(&random_seed_bytes[..8]);
        u64::from_ne_bytes(random_seed)
    }

    pub fn tick(&mut self) -> Result<()> {
        let active_clients = match self.coordinator.run_state {
            RunState::WaitingForMembers => {
                // Reset state flags
                self.is_warmup_first_tick = SmallBoolean::from(true);
                self.is_training_first_tick = SmallBoolean::from(true);

                let active_clients = self.clients_state.active_clients();
                msg!("Pending active clients: {}", active_clients.len());
                Some(active_clients)
            },
            _ => None,
        };

        msg!("Pre-tick run state: {}", self.coordinator.run_state);

        let clock: Clock = Clock::get()?;
        match self.coordinator.tick(
            active_clients,
            clock.unix_timestamp as u64,
            Self::get_random_seed(&clock),
        ) {
            Ok(TickResult::Ticked) => {
                if self.coordinator.is_warmup_just_starting()
                    && self.is_warmup_first_tick.is_true()
                {
                    msg!("New epoch just starting, save epoch rewards rate");
                    self.clients_state.current_epoch_rates =
                        self.clients_state.future_epoch_rates;
                    self.is_warmup_first_tick = SmallBoolean::from(false);
                } else if self.coordinator.is_training_just_starting()
                    && self.is_training_first_tick.is_true()
                {
                    msg!("New epoch just starting, save epoch active clients");
                    self.clients_state.next_active += 1;
                    self.is_training_first_tick = SmallBoolean::from(false);
                }
            },
            Ok(TickResult::EpochEnd(success)) => {
                msg!("Epoch end, sucecsss: {}", success);

                let mut i = 0;
                let mut j = 0;
                let finished_clients = &self.coordinator.epoch_state.clients;
                let exited_clients =
                    &self.coordinator.epoch_state.exited_clients;

                for client in self.clients_state.clients.iter_mut() {
                    if i < finished_clients.len()
                        && client.id == finished_clients[i].id
                    {
                        if finished_clients[i].state == ClientState::Healthy {
                            client.earned += self
                                .clients_state
                                .current_epoch_rates
                                .earning_rate;
                        }
                        i += 1;
                    }

                    if j < exited_clients.len()
                        && client.id == exited_clients[j].id
                    {
                        if exited_clients[j].state == ClientState::Ejected {
                            client.slashed += self
                                .clients_state
                                .current_epoch_rates
                                .slashing_rate;
                        }
                        j += 1;
                    }
                }
            },
            Err(err) => return err!(ProgramError::from(err)),
        };

        msg!("Post-tick run state: {}", self.coordinator.run_state);
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<()> {
        if let Err(err) = match paused {
            true => self.coordinator.pause(),
            false => {
                if !self.coordinator.config.check() {
                    return err!(ProgramError::ConfigSanityCheckFailed);
                }
                if !self.coordinator.model.check() {
                    return err!(ProgramError::ModelSanityCheckFailed);
                }

                if self.coordinator.run_state == RunState::Uninitialized {
                    // this is the only way to get out of RunState::Uninitialized
                    // by doing this we force the sanity checks on the config and model
                    // pass before starting the first step
                    self.coordinator.run_state = RunState::Paused;
                    // step 1 is the first valid step
                    self.coordinator.progress.step = 1;
                }
                self.coordinator.resume(Clock::get()?.unix_timestamp as u64)
            },
        } {
            return err!(ProgramError::from(err));
        }
        self.tick()
    }

    pub fn witness(&mut self, payer: &Pubkey, witness: Witness) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        let clock: Clock = Clock::get()?;
        self.coordinator
            .witness(
                id,
                witness,
                clock.unix_timestamp as u64,
                Self::get_random_seed(&clock),
            )
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;

        self.tick()
    }

    pub fn set_future_epoch_rates(
        &mut self,
        epoch_earning_rate: Option<u64>,
        epoch_slashing_rate: Option<u64>,
    ) -> Result<()> {
        if let Some(epoch_earning_rate) = epoch_earning_rate {
            self.clients_state.future_epoch_rates.earning_rate =
                epoch_earning_rate;
        }
        if let Some(epoch_slashing_rate) = epoch_slashing_rate {
            self.clients_state.future_epoch_rates.slashing_rate =
                epoch_slashing_rate;
        }
        Ok(())
    }

    pub fn update_coordinator_config_model(
        &mut self,
        config: Option<CoordinatorConfig<ClientId>>,
        model: Option<Model>,
    ) -> Result<()> {
        if self.coordinator.run_state == RunState::Finished {
            return err!(ProgramError::UpdateConfigFinished);
        } else if !self.coordinator.halted() {
            return err!(ProgramError::UpdateConfigNotHalted);
        }

        if let Some(config) = config {
            if !config.check() {
                return err!(ProgramError::ConfigSanityCheckFailed);
            }

            let _ = std::mem::replace(&mut self.coordinator.config, config);
        }

        if let Some(model) = model {
            if !model.check() {
                return err!(ProgramError::ModelSanityCheckFailed);
            }

            let _ = std::mem::replace(&mut self.coordinator.model, model);
        }

        Ok(())
    }

    pub fn get_active_clients(
        &self,
    ) -> SizedIterator<impl Iterator<Item = &ClientId>> {
        self.clients_state.active_clients()
    }

    pub fn join_run(&mut self, id: ClientId) -> Result<()> {
        let exisiting = match self
            .clients_state
            .clients
            .iter_mut()
            .find(|x| x.id.signer == id.signer)
        {
            Some(client) => {
                if client.id != id {
                    return err!(ProgramError::ClientIdMismatch);
                }
                client.id = id; // IMPORTANT. Equality is on wallet key but includes ephemeral p2p key
                client.active = self.clients_state.next_active;
                msg!("Exisiting client {} re-joined", id.signer);
                true
            },
            None => false,
        };

        if !exisiting
            && self
                .clients_state
                .clients
                .push(Client {
                    id,
                    staked: 0,
                    earned: 0,
                    slashed: 0,
                    active: self.clients_state.next_active,
                })
                .is_err()
        {
            return err!(ProgramError::ClientsFull);
        } else {
            msg!(
                "New client {} joined, {} total clients",
                id.signer,
                self.clients_state.clients.len()
            );
        }

        if !self.coordinator.halted() {
            self.tick()
        } else {
            Ok(())
        }
    }

    pub fn health_check(
        &mut self,
        payer: &Pubkey,
        checks: HealthChecks<ClientId>,
    ) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        self.coordinator
            .health_check(id, checks)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;
        self.tick()
    }
}
