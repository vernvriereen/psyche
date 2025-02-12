use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Lender {
    pub bump: u8,

    pub deposited_collateral_amount: u64,
    pub claimed_redeemable_amount: u64,
}

impl Lender {
    pub const SEEDS_PREFIX: &'static [u8] = b"Lender";

    pub fn space_with_discriminator() -> usize {
        8 + std::mem::size_of::<Lender>()
    }
}
