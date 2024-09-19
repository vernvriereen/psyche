use crate::{model::Model, traits::Backend};
use psyche_core::NodeIdentity;
use psyche_serde::derive_serialize;

#[cfg(target_os = "solana")]
use anchor_lang::prelude::*;
#[cfg(not(target_os = "solana"))]
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
const MAX_STRING_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[derive_serialize]
pub enum RunState {
    #[default]
    WaitingForMembers,
    Warmup,
    RoundStart,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct Client<I: NodeIdentity> {
    pub id: I,
    pub num_data_indicies: u32,
}

#[derive(Clone, Default, Debug)]
#[derive_serialize]
pub struct Round {
    pub height: u32,
    pub clients_len: u32,
    pub data_index: u64,
    pub random_seed: u64,
}

#[derive_serialize]
#[derive(Clone, Debug)]
pub struct Coordinator<T: NodeIdentity> {
    #[cfg_attr(target_os = "solana", max_len(MAX_STRING_LEN))]
    pub run_id: String,
    pub run_state: RunState,
    pub run_state_start_unix_timestamp: u64,

    pub warmup_time: u64,

    pub max_rounds: u32,
    pub max_round_time: u64,
    pub rounds: [Round; 4],
    pub rounds_head: u32,

    pub min_clients: u32,
    pub clients: Vec<Client<T>>,
    pub dropped_clients: Vec<Client<T>>,

    pub tick: u64,
    pub last_tick_unix_timestamp: u64,

    pub data_indicies_per_round: u32,
    pub verification_percent: u8,

    pub epoch: u32,
    pub step: u32,
    pub last_step_unix_timestamp: u64,

    pub model: Option<Model>,
}

impl<T: NodeIdentity> Default for Coordinator<T> {
    fn default() -> Self {
        Self {
            run_id: Default::default(),
            run_state: Default::default(),
            run_state_start_unix_timestamp: Default::default(),
            warmup_time: Default::default(),
            max_rounds: Default::default(),
            max_round_time: Default::default(),
            rounds: Default::default(),
            rounds_head: Default::default(),
            min_clients: 1,
            clients: Vec::new(),
            dropped_clients: Vec::new(),
            tick: Default::default(),
            last_tick_unix_timestamp: Default::default(),
            data_indicies_per_round: Default::default(),
            verification_percent: Default::default(),
            step: Default::default(),
            last_step_unix_timestamp: Default::default(),
            epoch: Default::default(),
            model: Default::default(),
        }
    }
}

impl<T: NodeIdentity> Coordinator<T> {
    pub fn tick(&mut self, backend: &dyn Backend<T>, unix_timestamp: u64, random_seed: u64) {
        match self.run_state {
            RunState::WaitingForMembers => self.waiting_for_members(backend, unix_timestamp),
            RunState::Warmup => self.warmup(unix_timestamp),
            RunState::RoundStart => self.round_start(unix_timestamp, random_seed),
        }
        self.tick += 1;
        self.last_tick_unix_timestamp = unix_timestamp;
    }

    fn waiting_for_members(&mut self, backend: &dyn Backend<T>, unix_timestamp: u64) {
        let clients = backend.select_new_clients();
        if clients.len() as u32 >= self.min_clients {
            self.clients = clients.into();
            self.rounds.fill(Round::empty());
            self.change_state(unix_timestamp, RunState::Warmup);
        }
    }

    fn warmup(&mut self, unix_timestamp: u64) {
        if unix_timestamp >= self.warmup_time + self.run_state_start_unix_timestamp {
            self.change_state(unix_timestamp, RunState::RoundStart);
        }
    }

    fn round_start(&mut self, unix_timestamp: u64, random_seed: u64) {
        if (self.clients.len() as u32) < self.min_clients {
            self.change_state(unix_timestamp, RunState::WaitingForMembers);
            return;
        }
        let (next_rounds_head, next_height) = if self.rounds_head == 0 && self.rounds[0].height == 0
        {
            // very first round, don't increment -- just start here
            (0usize, 0u32)
        } else {
            let current_round = &self.rounds[self.rounds_head as usize];
            if current_round.height == self.max_rounds - 1 {
                return;
            } else {
                (
                    (self.rounds_head + 1) as usize % self.rounds.len(),
                    current_round.height + 1,
                )
            }
        };
        let round = &mut self.rounds[next_rounds_head];
        self.rounds_head = next_rounds_head as u32;
        round.clients_len = self.clients.len() as u32;
        round.height = next_height;
        round.data_index += self.data_indicies_per_round as u64;
        round.random_seed = random_seed;
    }

    fn change_state(&mut self, unix_timestamp: u64, new_state: RunState) {
        self.run_state_start_unix_timestamp = unix_timestamp;
        self.run_state = new_state;
    }
}

impl Round {
    pub fn empty() -> Self {
        Self {
            height: 0,
            clients_len: 0,
            data_index: 0,
            random_seed: 0,
        }
    }
}
