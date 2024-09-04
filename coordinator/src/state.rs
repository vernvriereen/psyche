pub enum RunState {
    WaitingForMembers,
    Warmup,
    Running,
}

pub struct CoordinatorClient {
    pub id: String,
}

pub struct State {
    run_state: RunState,
}