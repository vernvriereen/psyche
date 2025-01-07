#![allow(unexpected_cfgs)]

mod committee_selection;
mod coordinator;
mod data_selection;
pub mod model;

pub use committee_selection::{
    Committee, CommitteeProof, CommitteeSelection, WitnessProof, COMMITTEE_SALT, WITNESS_SALT,
};
pub use coordinator::{
    Client, ClientState, Commitment, CoodinatorConfig, Coordinator, CoordinatorEpochState,
    CoordinatorError, HealthChecks, Round, RunState, TickResult, Witness, WitnessBloom,
    BLOOM_FALSE_RATE, NUM_STORED_ROUNDS, SOLANA_MAX_NUM_CLIENTS, SOLANA_MAX_NUM_WITNESSES,
    SOLANA_MAX_STRING_LEN,
};
pub use data_selection::{assign_data_for_state, get_batch_ids_for_round};