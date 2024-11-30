use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

#[cfg(target_os = "solana")]
pub trait NodeIdentity:
    Display
    + Debug
    + PartialEq
    + Eq
    + Hash
    + AsRef<[u8]>
    + Clone
    + Send
    + Sync
    + anchor_lang::Space
    + 'static
{
}

#[cfg(not(target_os = "solana"))]
pub trait NodeIdentity:
    Display + Debug + PartialEq + Eq + Hash + AsRef<[u8]> + Clone + Send + Sync + 'static
{
}
