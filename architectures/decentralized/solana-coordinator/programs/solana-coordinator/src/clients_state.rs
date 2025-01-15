use crate::{
    client::Client, program_error::ProgramError, ClientId, SOLANA_MAX_NUM_PENDING_CLIENTS,
    SOLANA_MAX_NUM_WHITELISTED_CLIENTS,
};

use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use psyche_core::{FixedVec, SizedIterator};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub struct ClientsState {
    pub whitelist: FixedVec<Pubkey, SOLANA_MAX_NUM_WHITELISTED_CLIENTS>,
    pub clients: FixedVec<Client, SOLANA_MAX_NUM_PENDING_CLIENTS>,
    pub next_active: u64,
}

unsafe impl Pod for ClientsState {}

impl ClientsState {
    pub fn active_clients(&self) -> SizedIterator<impl Iterator<Item = &ClientId>> {
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

    pub fn find_signer(&self, signer: &Pubkey) -> Result<&ClientId> {
        match self.clients.iter().find(|x| x.id.signer == *signer) {
            Some(client) => Ok(&client.id),
            None => err!(ProgramError::SignerNotAClient),
        }
    }
}
