use psyche_solana_mining_pool::state::Lender;
use solana_sdk::pubkey::Pubkey;

pub fn find_pda_lender(pool: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[Lender::SEEDS_PREFIX, pool.as_ref(), user.as_ref()],
        &psyche_solana_mining_pool::ID,
    )
    .0
}
