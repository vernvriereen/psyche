use anyhow::Result;
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

#[cfg(target_os = "solana")]
pub trait NodeIdentity:
    Display + Debug + PartialEq + Eq + Hash + AsRef<[u8]> + Clone + Send + Sync + anchor_lang::Space + 'static
{
    type PrivateKey: Send + Sync + Clone;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self>;
    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8>;

    fn get_p2p_public_key(&self) -> &[u8; 32];
}

#[cfg(not(target_os = "solana"))]
pub trait NodeIdentity:
    Display + Debug + PartialEq + Eq + Hash + AsRef<[u8]> + Clone + Send + Sync + 'static
{
    type PrivateKey: Send + Sync + Clone;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self>;
    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8>;

    fn get_p2p_public_key(&self) -> &[u8; 32];
}
