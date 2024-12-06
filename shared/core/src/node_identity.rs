use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

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
