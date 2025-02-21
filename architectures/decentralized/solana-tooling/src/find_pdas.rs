use psyche_solana_coordinator::bytes_from_string;
use psyche_solana_coordinator::CoordinatorInstance;
use psyche_solana_treasurer::run_identity_from_string;
use psyche_solana_treasurer::state::Participant;
use psyche_solana_treasurer::state::Run;
use solana_sdk::pubkey::Pubkey;

pub fn find_coordinator_instance(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[CoordinatorInstance::SEEDS_PREFIX, bytes_from_string(run_id)],
        &psyche_solana_coordinator::ID,
    )
    .0
}

pub fn find_run(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[Run::SEEDS_PREFIX, run_identity_from_string(run_id).as_ref()],
        &psyche_solana_treasurer::ID,
    )
    .0
}

pub fn find_participant(
    run: &Pubkey,
    user: &Pubkey,
) -> Pubkey {
    Pubkey::find_program_address(
        &[Participant::SEEDS_PREFIX, run.as_ref(), user.as_ref()],
        &psyche_solana_treasurer::ID,
    )
    .0
}
