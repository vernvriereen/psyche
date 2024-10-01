use crate::{protocol::NE, state::State, BroadcastMessage, Payload, NC};
use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::{BlobTicket, NetworkTUIState};
use psyche_watcher::{Backend, BackendWatcher};
use std::{borrow::BorrowMut, marker::PhantomData};
use tokio::{
    select,
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::error;

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    rx: Receiver<NetworkTUIState>,
    req_network_state: Sender<()>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, B)>,
    tui_state: NetworkTUIState,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, mut p2p: NC, identity: T, private_key: T::PrivateKey) -> Self {
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel::<NetworkTUIState>(10);
        let (req_network_state, mut got_req_network_state) = mpsc::channel(10);

        let join = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                let mut watcher = BackendWatcher::new(backend);
                let mut state = State::new(identity, private_key);
                let mut prev_ticket: Option<BlobTicket> = None;

                loop {
                    let step_result = select! {
                        _ = cancel.cancelled() => break,
                        Some(()) = got_req_network_state.recv() => Self::handle_network_state_request(&p2p, &tx).await.map(|_| None ),
                        res = watcher.borrow_mut().poll_next() => Ok(Some(res.map(|(c,cn)| (c, cn.clone())))),
                        res = p2p.poll_next() => Self::handle_p2p_poll(&mut state, &watcher, &mut p2p, res).await.map(|_| None),
                        res = state.poll_next() => Self::handle_state_poll(&mut state, &mut p2p, &mut prev_ticket, res).await.map(|_| None),
                    }?;

                    if let Some(watcher_res) = step_result {
                        Self::handle_watcher_poll(&mut state, &mut watcher, watcher_res).await?;
                    }
                }

                Ok(())
            }
        });

        Self {
            _t: Default::default(),
            cancel,
            req_network_state,
            rx,
            tui_state: Default::default(),
            join,
        }
    }

    async fn handle_network_state_request(p2p: &NC, tx: &Sender<NetworkTUIState>) -> Result<()> {
        tx.send(p2p.into()).await.map_err(|e| e.into())
    }

    async fn handle_watcher_poll(
        state: &mut State<T>,
        watcher: &mut BackendWatcher<T, B>,
        res: Result<(Option<Coordinator<T>>, Coordinator<T>)>,
    ) -> Result<()> {
        let (prev_state, new_state) = res?;
        let witness_send = state.process_new_state(&new_state, prev_state).await?;
        if let Some(witness) = witness_send {
            watcher.backend_mut().send_witness(witness).await
        } else {
            Ok(())
        }
    }

    async fn handle_p2p_poll(
        state: &mut State<T>,
        watcher: &BackendWatcher<T, B>,
        p2p: &mut NC,
        res: Result<Option<NE>>,
    ) -> Result<()> {
        match res {
            Ok(Some(event)) => match state.process_network_event(event, watcher)? {
                Some(download) => p2p.start_download(download).await,
                None => Ok(()),
            },
            Err(err) => Err(err),
            _ => Ok(()),
        }
    }

    async fn handle_state_poll(
        state: &mut State<T>,
        p2p: &mut NC,
        prev_ticket: &mut Option<BlobTicket>,
        res: Result<Option<(BroadcastMessage, Payload)>>,
    ) -> Result<()> {
        match res {
            Ok(Some((broadcast, payload))) => {
                if let Some(ticket) = prev_ticket.take() {
                    p2p.remove_downloadable(ticket).await?;
                }

                let new_ticket = p2p.add_downloadable(payload.clone()).await?;
                *prev_ticket = Some(new_ticket.clone());

                let mut broadcast = broadcast;
                broadcast.ticket = new_ticket;
                p2p.broadcast(&broadcast).await?;

                let identity = state.identity.clone();
                let hash = broadcast.ticket.hash();
                state.handle_broadcast(&identity, broadcast)?;
                state.handle_payload(hash, payload)
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        select! {
            Some(tui_state) = self.rx.recv() => {
                self.tui_state = tui_state;
            },
            res = &mut self.join => if let Err(err) = res? {
                error!("Client ending with error: {err}");
                return Err(err);
            }
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub async fn network_tui_state(&self) -> NetworkTUIState {
        let _ = self.req_network_state.send(()).await;
        self.tui_state.clone()
    }
}
