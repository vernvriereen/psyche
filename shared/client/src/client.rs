use crate::{state::State, BroadcastMessage, NC};
use anyhow::Result;
use psyche_core::NodeIdentity;
use psyche_network::{BlobTicket, NetworkTUIState};
use psyche_watcher::{Backend, BackendWatcher};
use tokio::select;

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    watcher: BackendWatcher<T, B>,
    p2p: NC,
    state: State<T>,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, p2p: NC, identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            watcher: BackendWatcher::new(backend),
            p2p,
            state: State::new(identity, private_key),
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        let mut ticket: Option<BlobTicket> = None;
        select! {
            res = self.watcher.poll_next() => {
                match res {
                    Ok((prev_state, state)) => {
                        self.state.process_new_state(state, prev_state).await?
                    }
                    Err(err) => { return Err(err); }
                }
            },
            res = self.p2p.poll_next() => match res {
                Ok(Some(event)) => {
                    self.state.process_network_event(event, &self.watcher).await?;
                },
                Err(err) => { return Err(err); }
                _ => {},
            },
            res = self.state.poll_next() => match res {
                Ok(Some((committee, payload))) => {
                    if let Some(ticket) = ticket {
                        self.p2p.remove_downloadable(ticket).await?;
                    }
                    let step = payload.step;
                    ticket = Some(self.p2p.add_downloadable(payload).await?);
                    self.p2p.broadcast(&BroadcastMessage { step, ticket: ticket.clone().unwrap(), committee }).await?;
                },
                Ok(None) => {},
                Err(err) => { return Err(err); }
            },
        }
        Ok(())
    }

    pub fn network_tui_state(&self) -> NetworkTUIState {
        (&self.p2p).into()
    }
}
