use crate::{NC, state::State};
use anyhow::Result;
use psyche_core::NodeIdentity;
use psyche_network::NetworkTUIState;
use psyche_watcher::{Backend, BackendWatcher};
use tokio::{select, sync::Mutex};


pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    watcher: BackendWatcher<T, B>,
    p2p: NC,
    state: Mutex<State<T>>,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, p2p: NC, identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            watcher: BackendWatcher::new(backend),
            p2p,
            state: Mutex::new(State::new(identity, private_key)),
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        select! {
            res = self.watcher.poll_next() => {
                match res {
                    Ok((prev_state, state)) => {
                        self.state.lock().await.process_new_state(state, prev_state).await?
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
        Ok(())
    }

    pub fn network_tui_state(&self) -> NetworkTUIState {
        (&self.p2p).into()
    }
}
