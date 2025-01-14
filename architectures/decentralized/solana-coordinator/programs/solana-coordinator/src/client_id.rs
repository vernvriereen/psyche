use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use psyche_core::NodeIdentity;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(
    Debug,
    InitSpace,
    Copy,
    Clone,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    Default,
    Zeroable,
    Pod,
)]
pub struct ClientId {
    pub signer: Pubkey,
    pub p2p_identity: [u8; 32],
}

impl AsRef<[u8]> for ClientId {
    fn as_ref(&self) -> &[u8] {
        self.signer.as_ref()
    }
}

impl std::hash::Hash for ClientId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signer.hash(state);
    }
}

impl std::cmp::PartialEq for ClientId {
    fn eq(&self, other: &Self) -> bool {
        self.signer == other.signer
    }
}

impl std::cmp::Eq for ClientId {}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.signer)
    }
}

impl NodeIdentity for ClientId {
    fn get_p2p_public_key(&self) -> &[u8; 32] {
        &self.p2p_identity
    }
}

impl ClientId {
    pub fn new(owner: Pubkey, p2p_identity: [u8; 32]) -> Self {
        Self {
            signer: owner,
            p2p_identity,
        }
    }
}
