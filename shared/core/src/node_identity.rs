use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use bytemuck::Zeroable;
use serde::{de::DeserializeOwned, Serialize};

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
    + Zeroable
    + Default
    + Serialize
    + AnchorDeserialize
    + AnchorSerialize
    + DeserializeOwned
    + 'static
{
}
