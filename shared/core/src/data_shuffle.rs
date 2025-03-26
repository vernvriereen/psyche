use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    TS,
)]
#[repr(C)]
pub enum Shuffle {
    DontShuffle,
    Seeded([u8; 32]),
}

impl Default for Shuffle {
    fn default() -> Self {
        Self::DontShuffle
    }
}
