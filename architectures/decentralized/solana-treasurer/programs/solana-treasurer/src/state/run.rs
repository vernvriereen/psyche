use std::mem::size_of;

use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Run {
    pub bump: u8,

    pub identity: Pubkey,
    pub authority: Pubkey,

    pub collateral_mint: Pubkey,
    pub total_funded_collateral_amount: u64,
}

impl Run {
    pub const SEEDS_PREFIX: &'static [u8] = b"Run";
    pub fn space_with_discriminator() -> usize {
        8 + size_of::<Run>()
    }
}
