use anchor_lang::prelude::*;
use bytemuck::Zeroable;
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
    Zeroable
)]
pub struct ClientId {
    pub owner: Pubkey,
    pub p2p_identity: [u8; 32],
}

impl AsRef<[u8]> for ClientId {
    fn as_ref(&self) -> &[u8] {
        self.owner.as_ref()
    }
}

impl std::hash::Hash for ClientId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
    }
}

impl std::cmp::PartialEq for ClientId {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
    }
}

impl std::cmp::Eq for ClientId {}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.owner)
    }
}

impl NodeIdentity for ClientId {}
