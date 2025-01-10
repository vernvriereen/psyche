use solana_coordinator::bytes_from_string;
use solana_sdk::pubkey::Pubkey;

pub fn find_pda_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[b"coordinator", bytes_from_string(run_id)],
        &solana_coordinator::ID,
    )
    .0
}
