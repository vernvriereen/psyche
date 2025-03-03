use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Authorization {
    pub bump: u8,

    pub grantor: Pubkey,
    pub grantee: Pubkey,
    pub scope: Vec<u8>,

    pub active: bool,
    pub delegates: Vec<Pubkey>,
}

impl Authorization {
    pub const SEEDS_PREFIX: &'static [u8] = b"Authorization";

    pub fn space_with_discriminator(
        scope_len: usize,
        delegates_len: usize,
    ) -> usize {
        8 + std::mem::size_of::<bool>()
            + std::mem::size_of::<Pubkey>()
            + std::mem::size_of::<Pubkey>()
            + (4 + scope_len * std::mem::size_of::<u8>())
            + std::mem::size_of::<bool>()
            + (4 + delegates_len * std::mem::size_of::<Pubkey>())
    }
}
