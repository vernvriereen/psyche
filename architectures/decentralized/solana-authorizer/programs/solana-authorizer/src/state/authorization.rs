use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Authorization {
    pub bump: u8,

    pub grantor: Pubkey,
    pub grantee: Pubkey,

    pub scope: Vec<u8>,
    pub delegates: Vec<Pubkey>,
}

impl Authorization {
    pub const SEEDS_PREFIX: &'static [u8] = b"Authorization";

    pub fn space_with_discriminator(
        scope_len: usize,
        delegates_len: usize,
    ) -> usize {
        8 + 1 + 32 + 32 + 4 + scope_len + 4 + delegates_len * 32
    }
}
