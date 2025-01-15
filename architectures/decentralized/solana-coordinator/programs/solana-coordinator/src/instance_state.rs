use crate::{clients_state::ClientsState, client::Client, ClientId, ProgramError};

use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use psyche_coordinator::{
    model::Model, ClientState, Coordinator, CoordinatorConfig, RunState, TickResult, Witness,
};
use psyche_core::sha256v;

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub struct CoordinatorInstanceState {
    pub coordinator: Coordinator<ClientId>,
    pub clients_state: ClientsState,
}

unsafe impl Pod for CoordinatorInstanceState {}

impl CoordinatorInstanceState {
    pub fn tick(&mut self) -> Result<()> {
        let clock: Clock = Clock::get()?;
        let random_seed_bytes = sha256v(&[
            &clock.unix_timestamp.to_ne_bytes(),
            &clock.slot.to_ne_bytes(),
        ]);

        let mut random_seed: [u8; 8] = [0; 8];
        random_seed.copy_from_slice(&random_seed_bytes[..8]);

        let active_clients = match self.coordinator.run_state {
            RunState::WaitingForMembers => Some(self.clients_state.active_clients()),
            _ => None,
        };

        match self.coordinator.tick(
            active_clients,
            clock.unix_timestamp as u64,
            u64::from_ne_bytes(random_seed),
        ) {
            Ok(TickResult::Ticked) => Ok(()),
            Ok(TickResult::EpochEnd(_)) => {
                self.clients_state.next_active += 1;

                let mut i = 0;
                let mut j = 0;
                let finished_clients = &self.coordinator.epoch_state.clients;
                let exited_clients = &self.coordinator.epoch_state.exited_clients;

                for client in self.clients_state.clients.iter_mut() {
                    if i < finished_clients.len() && client.id == finished_clients[i].id {
                        if finished_clients[i].state == ClientState::Healthy {
                            client.earned += 1;
                        }
                        i += 1;
                    }

                    if j < exited_clients.len() && client.id == exited_clients[j].id {
                        if exited_clients[j].state == ClientState::Ejected {
                            client.slashed += 1;
                        }
                        j += 1;
                    }
                }

                Ok(())
            }
            Err(err) => {
                err!(ProgramError::from(err))
            }
        }
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
                    self.coordinator.run_state = RunState::Paused;
                    // resume() copies the previous epoch's progress
                    // step 1 is the first valid step
                    self.coordinator.prev_epoch_progress.step = 1;
                }
                self.coordinator.resume(Clock::get()?.unix_timestamp as u64)
            }
        } {
            return err!(ProgramError::from(err));
        }
        self.tick()
    }

    pub fn witness(&mut self, payer: &Pubkey, witness: Witness) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        self.coordinator
            .witness(id, witness, Clock::get()?.unix_timestamp as u64)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;
        self.tick()
    }

    pub fn set_whitelist(&mut self, clients: Vec<Pubkey>) -> Result<()> {
        self.clients_state.whitelist.clear();
        self.clients_state
            .whitelist
            .extend(clients.into_iter())
            .map_err(|_| ProgramError::CouldNotSetWhitelist)?;
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

    pub fn join_run(&mut self, id: ClientId) -> Result<()> {
        if !self.clients_state.whitelist.is_empty() {
            if !self.clients_state.whitelist.iter().any(|x| x == &id.signer) {
                return err!(ProgramError::NotInWhitelist);
            }
        }

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
                true
            }
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
        }

        self.tick()
    }
}
