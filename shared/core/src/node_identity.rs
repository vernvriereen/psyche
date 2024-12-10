use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use bytemuck::Pod;

pub trait NodeIdentity:
    Display
    + Copy
    + Debug
    + PartialEq
    + Eq
    + Hash
    + AsRef<[u8]>
    + Clone
    + Send
    + Sync
    + anchor_lang::Space
    + Default
    + 'static
{
}
