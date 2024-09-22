use crate::NC;
use anyhow::Result;
use psyche_coordinator::RunState;
use psyche_core::NodeIdentity;
use psyche_network::NetworkTUIState;
use psyche_watcher::{Backend, BackendWatcher};
use tokio::select;

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    watcher: BackendWatcher<T, B>,
    p2p: NC,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, p2p: NC) -> Self {
        Self {
            watcher: BackendWatcher::new(backend),
            p2p,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                res = self.watcher.poll_next() => {
                    match res {
                        Ok((_prev_state, state)) => match state.run_state {
                            RunState::WaitingForMembers => {},
                            RunState::Warmup => {},
                            RunState::RoundStart => {},
                        }
                        Err(err) => { return Err(err); }
                    }
                },
                res = self.p2p.poll_next() => match res {
                    Ok(Some(_event)) => {

                    },
                    Err(err) => { return Err(err); }
                    _ => {},
                }
            }
        }
    }

    pub fn network_tui_state(&self) -> NetworkTUIState {
        (&self.p2p).into()
    }
}
