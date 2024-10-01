use crate::{state::State, BroadcastMessage, Payload, NC};
use anyhow::Result;
use psyche_coordinator::Witness;
use psyche_core::NodeIdentity;
use psyche_network::{BlobTicket, NetworkTUIState};
use psyche_watcher::{Backend, BackendWatcher};
use tokio::select;

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    watcher: BackendWatcher<T, B>,
    p2p: NC,
    state: State<T>,
    sharing: Option<BlobTicket>,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, p2p: NC, identity: T, private_key: T::PrivateKey) -> Self {
        Self {
            watcher: BackendWatcher::new(backend),
            p2p,
            state: State::new(identity, private_key),
            sharing: None,
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        let mut p2p_send: Option<(BroadcastMessage, Payload)> = None;
        let mut witness_send: Option<Witness> = None;
        select! {
            res = self.watcher.poll_next() => {
                match res {
                    Ok((prev_state, state)) => {
                        witness_send = self.state.process_new_state(state, prev_state).await?;
                    }
                    Err(err) => { return Err(err); }
                }
            },
            res = self.p2p.poll_next() => match res {
                Ok(Some(event)) => {
                    self.state.process_network_event(event, &self.watcher, &mut self.p2p).await?;
                },
                Err(err) => { return Err(err); }
                _ => {},
            },
            res = self.state.poll_next() => {
                p2p_send = res?;
            },
        }
        if let Some((mut broadcast, payload)) = p2p_send {
            let sharing = std::mem::take(&mut self.sharing);
            if let Some(ticket) = sharing {
                self.p2p.remove_downloadable(ticket).await?;
            }
            self.sharing = Some(self.p2p.add_downloadable(payload.clone()).await?);
            broadcast.ticket = self.sharing.clone().unwrap();
            self.p2p.broadcast(&broadcast).await?;
            let identity = self.state.identity.clone();
            let hash = broadcast.ticket.hash();
            self.state
                .handle_broadcast(&identity, broadcast, &mut self.p2p)
                .await?;
            self.state.handle_payload(hash, payload)?;
        }
        if let Some(witness) = witness_send {
            self.watcher.mut_backend().send_witness(witness).await?;
        }
        Ok(())
    }

    pub fn network_tui_state(&self) -> NetworkTUIState {
        (&self.p2p).into()
    }
}
