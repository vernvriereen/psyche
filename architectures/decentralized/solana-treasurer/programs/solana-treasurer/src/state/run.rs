use std::mem::size_of;

use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Run {
    pub bump: u8,
    pub identity: [u8; 32],
    pub authority: Pubkey,
    pub collateral_mint: Pubkey,
}

impl Run {
    pub fn space_with_discriminator() -> usize {
        8 + size_of::<Run>()
    }
}

impl Run {
    pub const SEED_PREFIX: &'static [u8] = b"Run";
}
