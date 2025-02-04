use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Pool {
    pub bump: u8,

    pub index: u64,
    pub authority: Pubkey,

    pub collateral_mint: Pubkey,
    pub max_deposit_collateral_amount: u64,
    pub total_deposited_collateral_amount: u64,
    pub total_extracted_collateral_amount: u64,

    pub claiming_enabled: bool,
    pub redeemable_mint: Pubkey,
    pub total_claimed_redeemable_amount: u64,

    pub metadata: PoolMetadata,
}

#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PoolMetadata {
    pub length: u16,
    pub bytes: [u8; PoolMetadata::BYTES],
}

impl Pool {
    pub const SEEDS_PREFIX: &'static [u8] = b"Pool";

    pub fn space_with_discriminator() -> usize {
        8 + std::mem::size_of::<Pool>()
    }
}

impl PoolMetadata {
    pub const BYTES: usize = 400;
}
