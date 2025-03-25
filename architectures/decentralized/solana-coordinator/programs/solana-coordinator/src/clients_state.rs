use anchor_lang::prelude::*;

use crate::{
    client::Client, program_error::ProgramError, ClientId,
    SOLANA_MAX_NUM_PENDING_CLIENTS,
};

use bytemuck::{Pod, Zeroable};
use psyche_core::{FixedVec, SizedIterator};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Debug,
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
pub struct ClientsState {
    pub clients: FixedVec<Client, { SOLANA_MAX_NUM_PENDING_CLIENTS }>,
    pub next_active: u64,
    pub current_epoch_rates: ClientsEpochRates,
    pub future_epoch_rates: ClientsEpochRates,
}

#[derive(
    Debug,
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
pub struct ClientsEpochRates {
    pub earning_rate: u64,
    pub slashing_rate: u64,
}

unsafe impl Pod for ClientsState {}

impl ClientsState {
    pub fn purge_inactive_clients(&mut self) {
        let active_clients = self
            .clients
            .into_iter()
            .filter(|client| client.active == self.next_active)
            .collect::<Vec<_>>();
        self.clients.clear();
        self.clients.extend(active_clients).unwrap();
    }

    pub fn get_active_clients_ids(
        &self,
    ) -> SizedIterator<impl Iterator<Item = &ClientId>> {
        let mut size = 0;
        for x in self.clients.iter() {
            if x.active == self.next_active {
                size += 1;
            }
        }
        let iter = self.clients.iter().filter_map(move |x| {
            match x.active == self.next_active {
                true => Some(&x.id),
                false => None,
            }
        });
        SizedIterator::new(iter, size)
    }

    pub fn find_signer(&self, signer: &Pubkey) -> Result<&ClientId> {
        match self.clients.iter().find(|x| x.id.signer == *signer) {
            Some(client) => Ok(&client.id),
            None => err!(ProgramError::SignerNotAClient),
        }
    }
}
