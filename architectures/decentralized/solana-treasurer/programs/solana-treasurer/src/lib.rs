pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("77mYTtUnEzSYVoG1JtWCjKAdakSvYDkdPPy8DoGqr5RP");

pub fn find_run(run_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Run::SEEDS_PREFIX,
            run_identity_from_string(run_id).as_ref(),
        ],
        &crate::ID,
    )
    .0
}

pub fn find_participant(run: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Participant::SEEDS_PREFIX,
            run.as_ref(),
            user.as_ref(),
        ],
        &crate::ID,
    )
    .0
}

pub fn run_identity_from_string(string: &str) -> Pubkey {
    let mut bytes = vec![];
    bytes.extend_from_slice(string.as_bytes());
    bytes.resize(32, 0);
    Pubkey::new_from_array(bytes.try_into().unwrap())
}

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn run_create(
        context: Context<RunCreateAccounts>,
        params: RunCreateParams,
    ) -> Result<()> {
        run_create_processor(context, params)
    }

    pub fn run_top_up(
        context: Context<RunTopUpAccounts>,
        params: RunTopUpParams,
    ) -> Result<()> {
        run_top_up_processor(context, params)
    }

    pub fn run_update(
        context: Context<RunUpdateAccounts>,
        params: RunUpdateParams,
    ) -> Result<()> {
        run_update_processor(context, params)
    }

    pub fn run_authorize(
        context: Context<RunAuthorizeAccounts>,
        params: RunAuthorizeParams,
    ) -> Result<()> {
        run_authorize_processor(context, params)
    }

    pub fn participant_create(
        context: Context<ParticipantCreateAccounts>,
        params: ParticipantCreateParams,
    ) -> Result<()> {
        participant_create_processor(context, params)
    }

    pub fn participant_claim(
        context: Context<ParticipantClaimAccounts>,
        params: ParticipantClaimParams,
    ) -> Result<()> {
        participant_claim_processor(context, params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("Invalid parameter")]
    InvalidParameter,
}
