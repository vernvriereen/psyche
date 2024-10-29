use crate::{
    protocol::NE, state::{State, ToSend}, BroadcastMessage, ClientTUIState, WandBInfo, NC
};
use anyhow::Result;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::NetworkTUIState;
use psyche_watcher::{Backend, BackendWatcher};
use std::{borrow::BorrowMut, marker::PhantomData, path::PathBuf, sync::Arc};
use tokio::{
    select,
    sync::{
        watch::{self, Receiver},
        Notify,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NodeIdentity, B: Backend<T> + 'static> {
    rx: Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, B)>,
}

type CoordinatorUpdate<T> = (Option<Coordinator<T>>, Coordinator<T>);

impl<T: NodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    // todo: refactor into a struct
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: B,
        mut p2p: NC,
        identity: T,
        private_key: T::PrivateKey,
        data_parallelism: usize,
        tensor_parallelism: usize,
        eval_tasks: Vec<psyche_eval::Task>,
        eval_task_max_docs: Option<usize>,
        micro_batch_size: Option<usize>,
        write_gradients_dir: Option<PathBuf>,
        checkpoint_dir: Option<PathBuf>,
        hub_repo: Option<String>,
        hub_token: Option<String>,
        wandb_info: Option<WandBInfo>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let (tx, rx) = watch::channel::<TUIStates>(Default::default());
        let req_tui_state = Arc::new(Notify::new());

        let join = tokio::spawn({
            let cancel = cancel.clone();
            let req_tui_state = req_tui_state.clone();
            async move {
                let mut watcher = BackendWatcher::new(backend);
                let mut state = State::new(
                    identity,
                    private_key,
                    data_parallelism,
                    tensor_parallelism,
                    eval_tasks,
                    eval_task_max_docs,
                    micro_batch_size,
                    write_gradients_dir,
                    checkpoint_dir,
                    hub_repo,
                    hub_token,
                    wandb_info,
                );
                let clear_uploads = state.get_clear_downloads_notification();

                loop {
                    let step_result: std::result::Result<
                        Option<CoordinatorUpdate<T>>,
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
            Ok(Some(event)) => match state.process_network_event(event, watcher).await? {
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
                debug!(
                    "Broadcasting payload hash 0x{}",
                    hex::encode(new_ticket.hash()),
                );

                let mut broadcast = broadcast;
                match broadcast.borrow_mut() {
                    BroadcastMessage::TrainingResult(training_result) => {
                        training_result.ticket = new_ticket.clone()
                    }
                    BroadcastMessage::PeerAnnouncement(peer_announcement) => {
                        peer_announcement.ticket = new_ticket.clone();
                    }
                }

                p2p.broadcast(&broadcast).await?;

                let identity = state.identity.clone();
                let hash = new_ticket.hash();
                state.handle_broadcast(&identity, broadcast)?;
                state.handle_payload(hash, payload).await
            }
            ToSend::Witness(witness) => watcher.backend_mut().send_witness(witness).await,
            ToSend::HealthCheck(health_checks) => {
                watcher.backend_mut().send_health_check(health_checks).await
            }
            ToSend::Nothing => Ok(()),
            ToSend::Checkpoint(checkpoint) => {
                watcher.backend_mut().send_checkpoint(checkpoint).await
            }
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
        self.req_tui_state.notify_one();
        self.rx.borrow().clone()
    }
}
