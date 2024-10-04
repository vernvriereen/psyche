use crate::{
    protocol::NE,
    state::{State, ToSend},
    ClientTUIState, NC,
};
use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::NetworkTUIState;
use psyche_watcher::{Backend, BackendWatcher};
use std::{borrow::BorrowMut, marker::PhantomData, sync::Arc};
use tokio::{
    select,
    sync::{
        watch::{self, Receiver},
        Notify,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    rx: Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, B)>,
}

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    pub fn new(backend: B, mut p2p: NC, identity: T, private_key: T::PrivateKey) -> Self {
        let cancel = CancellationToken::new();
        let (tx, rx) = watch::channel::<TUIStates>(Default::default());
        let req_tui_state = Arc::new(Notify::new());

        let join = tokio::spawn({
            let cancel = cancel.clone();
            let req_tui_state = req_tui_state.clone();
            async move {
                let mut watcher = BackendWatcher::new(backend);
                let mut state = State::new(identity, private_key);
                let clear_uploads = state.get_clear_downloads_notification();

                loop {
                    let step_result: std::result::Result<
                        Option<(Option<Coordinator<T>>, Coordinator<T>)>,
                        anyhow::Error,
                    > = select! {
                        _ = cancel.cancelled() => break,
                        _ = req_tui_state.notified() => {
                            let network_tui_state = (&p2p).into();
                            let client_tui_state = (&state).into();
                            tx.send((client_tui_state, network_tui_state)).map_err(|e| e.into()).map(|_| None)
                        },
                        res = watcher.borrow_mut().poll_next() => res.map(|(c,cn)| Some((c, cn.clone()))),
                        res = p2p.poll_next() => Self::handle_p2p_poll(&mut state, &watcher, &mut p2p, res).await.map(|_| None),
                        res = state.poll_next() => Self::handle_state_poll(&mut state, &mut p2p, &mut watcher, res?).await.map(|_| None),
                        _ = clear_uploads.notified() => Self::handle_clear_uploads(&mut p2p).await.map(|_| None),
                    };

                    if let Some(watcher_res) = step_result? {
                        Self::handle_watcher_poll(&mut state, &mut watcher, watcher_res).await?;
                    }
                }

                Ok(())
            }
        });

        Self {
            _t: Default::default(),
            cancel,
            req_tui_state,
            rx,
            join,
        }
    }

    async fn handle_watcher_poll(
        state: &mut State<T>,
        watcher: &mut BackendWatcher<T, B>,
        res: (Option<Coordinator<T>>, Coordinator<T>),
    ) -> Result<()> {
        let (prev_state, new_state) = res;
        match state.process_new_state(&new_state, prev_state).await? {
            Some(ToSend::Witness(witness)) => watcher.backend_mut().send_witness(witness).await,
            None => Ok(()),
            _ => todo!(),
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
        watcher: &mut BackendWatcher<T, B>,
        res: ToSend,
    ) -> Result<()> {
        match res {
            ToSend::Broadcast((broadcast, payload)) => {
                let new_ticket = p2p.add_downloadable(payload.clone()).await?;
                info!(
                    "Broadcasting payload hash 0x{} for commitment 0x{}",
                    hex::encode(new_ticket.hash()),
                    hex::encode(broadcast.commitment)
                );

                let mut broadcast = broadcast;
                broadcast.ticket = new_ticket;
                p2p.broadcast(&broadcast).await?;

                let identity = state.identity.clone();
                let hash = broadcast.ticket.hash();
                state.handle_broadcast(&identity, broadcast)?;
                state.handle_payload(hash, payload)
            }
            ToSend::Witness(witness) => watcher.backend_mut().send_witness(witness).await,
            ToSend::HealthCheck(health_checks) => {
                watcher.backend_mut().send_health_check(health_checks).await
            }
            ToSend::Nothing => Ok(()),
        }
    }

    async fn handle_clear_uploads(p2p: &mut NC) -> Result<()> {
        for blob in p2p.currently_sharing_blobs().clone() {
            p2p.remove_downloadable(blob).await?;
        }
        Ok(())
    }

    pub async fn process(&mut self) -> Result<()> {
        select! {
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

    pub async fn tui_states(&self) -> TUIStates {
        let _ = self.req_tui_state.notify_one();
        self.rx.borrow().clone()
    }
}
