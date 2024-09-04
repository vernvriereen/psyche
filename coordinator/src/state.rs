use crate::client::Client;

pub enum RunState {
    WaitingForMembers,
    Warmup,
    RoundStart,
}

pub struct PrevRound {
    pub data_index: u64,
    pub clients_len: u32,
}

pub struct State<I> {
    pub run_state: RunState,
    pub run_state_start_unix_timestamp: u64,

    pub warmup_time: u64,

    pub round: u32,
    pub max_rounds: u32,
    pub max_round_time: u64,
    pub prev_round_info: [PrevRound; 4],

    pub min_clients: u32,
    pub clients: Vec<Client<I>>,
    pub dropped_clients: Vec<Client<I>>,

    pub last_step_unix_timestamp: u64,

    pub data_index: u64,
}