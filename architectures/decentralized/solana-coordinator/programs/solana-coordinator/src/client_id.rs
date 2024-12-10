use anchor_lang::prelude::*;
use psyche_core::NodeIdentity;
use serde::{Deserialize, Serialize};

#[account(zero_copy)]
#[repr(C)]
#[derive(Debug, InitSpace, AnchorSerialize, AnchorDeserialize, Default)]
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

impl Serialize for ClientId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer {
        unimplemented!()
    }
}

impl Deserialize for ClientId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
        where
            D: serde::Deserializer<'de> {
        unimplemented!()
    }
}
