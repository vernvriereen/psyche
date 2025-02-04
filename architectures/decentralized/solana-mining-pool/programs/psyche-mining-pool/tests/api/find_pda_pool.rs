use psyche_mining_pool::state::Pool;
use solana_sdk::pubkey::Pubkey;

pub fn find_pda_pool(pool_index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[Pool::SEEDS_PREFIX, &pool_index.to_le_bytes()],
        &psyche_mining_pool::ID,
    )
    .0
}
