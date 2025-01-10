use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
)]
#[repr(C)]
pub enum Shuffle {
    DontShuffle,
    Seeded([u8; 32]),
}
