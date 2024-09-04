use crate::{
    backend::Backend,
    client::Client,
    state::{RunState, State},
};

pub struct Coordinator<B: Backend<I>, I> {
    unix_timestamp: u64,
    state: State<I>,
    backend: B,
}

impl<B: Backend<I>, I> Coordinator<B, I>
where
    I: Clone,
{
    pub fn step(mut self) -> State<I> {
        match self.state.run_state {
            RunState::WaitingForMembers => self.waiting_for_members(),
            RunState::Warmup => self.warmup(),
            RunState::RoundStart => self.round_start(),
        }
        self.state.last_step_unix_timestamp = self.unix_timestamp;
        self.state
    }

    fn waiting_for_members(&mut self) {
        let clients = self.backend.select_new_clients();
        if clients.len() as u32 >= self.state.min_clients {
            self.state.clients = clients
                .into_iter()
                .map(|id| Client { id: id.clone() })
                .collect();
            self.state.run_state_start_unix_timestamp = self.unix_timestamp;
            self.state.run_state = RunState::Warmup;
        }
    }

    fn warmup(&mut self) {
        if self.unix_timestamp >= self.state.warmup_time + self.state.run_state_start_unix_timestamp {
            self.state.run_state_start_unix_timestamp = self.unix_timestamp;
            self.state.run_state = RunState::RoundStart;
        }
    }

    fn round_start(&mut self) {
    }

}
