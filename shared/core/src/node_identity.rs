use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
    + Serialize
    + AnchorDeserialize
    + AnchorSerialize
    + DeserializeOwned
    + 'static
{
}
