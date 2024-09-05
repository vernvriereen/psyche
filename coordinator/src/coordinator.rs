use crate::backend::Backend;

pub enum RunState {
    WaitingForMembers,
    Warmup,
    RoundStart,
}

pub struct Client<I> {
    pub id: I,
}

#[derive(Clone)]
pub struct Round {
    pub height: u32,
    pub clients_len: u32,
    pub data_index: u64,
    pub random_seed: u64,
}

pub struct Coordinator<I> {
    pub run_state: RunState,
    pub run_state_start_unix_timestamp: u64,

    pub warmup_time: u64,

    pub max_rounds: u32,
    pub max_round_time: u64,
    pub rounds: [Round; 4],
    pub rounds_head: u32,

    pub min_clients: u32,
    pub clients: Vec<Client<I>>,
    pub dropped_clients: Vec<Client<I>>,

    pub last_step_unix_timestamp: u64,

    pub data_indicies_per_round: u32,
    pub verification_percent: u8,

    pub epoch: u32,
}

impl<I> Coordinator<I>
where
    I: Clone + ToString,
{
    pub fn step(mut self, backend: &dyn Backend<I>, unix_timestamp: u64, random_seed: u64) -> Self {
        match self.run_state {
            RunState::WaitingForMembers => self.waiting_for_members(backend, unix_timestamp),
            RunState::Warmup => self.warmup(unix_timestamp),
            RunState::RoundStart => self.round_start(unix_timestamp, random_seed),
        }
        self.last_step_unix_timestamp = unix_timestamp;
        self
    }

    fn waiting_for_members(&mut self, backend: &dyn Backend<I>, unix_timestamp: u64) {
        let clients = backend.select_new_clients();
        if clients.len() as u32 >= self.min_clients {
            self.clients = clients
                .into_iter()
                .map(|id| Client { id: id.clone() })
                .collect();
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
