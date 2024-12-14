use crate::{
    state::{DistroBroadcastAndPayload, RunManager},
    ClientTUIState, RunInitConfig, RunInitConfigAndIO, TrainingResult, NC,
};
use anyhow::{Error, Result};
use psyche_coordinator::RunState;
use psyche_network::{
    DownloadComplete, NetworkConnection, NetworkEvent, NetworkTUIState, Networkable,
    NetworkableNodeIdentity,
};
use psyche_watcher::{Backend, BackendWatcher};
use wandb::DataValue;

use std::{collections::HashMap, marker::PhantomData, sync::Arc};
use tokio::{
    select,
    sync::{
        mpsc,
        watch::{self, Receiver},
        Notify,
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NetworkableNodeIdentity, B: Backend<T> + 'static> {
    rx: Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, B)>,
}

const MAX_DOWNLOAD_RETRIES: usize = 3;

impl<T: NetworkableNodeIdentity, B: Backend<T> + 'static> Client<T, B> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(backend: B, mut p2p: NC, init_config: RunInitConfig<T>) -> Self {
        let cancel = CancellationToken::new();
        let (tx, rx) = watch::channel::<TUIStates>(Default::default());
        let req_tui_state = Arc::new(Notify::new());

        let identity = init_config.identity.clone();
        let join = tokio::spawn({
            let cancel = cancel.clone();
            let req_tui_state = req_tui_state.clone();
            async move {
                let mut watcher = BackendWatcher::new(backend);

                // From Run
                let (tx_witness, mut rx_witness) = mpsc::unbounded_channel();
                let (tx_health_check, mut rx_health_check) = mpsc::unbounded_channel();
                let (tx_checkpoint, mut rx_checkpoint) = mpsc::unbounded_channel();
                let (tx_distro_result, mut rx_distro_result) = mpsc::unbounded_channel();
                let (tx_request_download, mut rx_request_download) = mpsc::unbounded_channel();

                let mut run = RunManager::<T>::new(RunInitConfigAndIO {
                    init_config,

                    tx_witness,
                    tx_health_check,
                    tx_checkpoint,
                    tx_distro_result,
                    tx_request_download,
                });

                let mut retried_downloads: HashMap<psyche_network::Hash, usize> = HashMap::new();
                loop {
                    select! {
                        _ = cancel.cancelled() => {
                            break;
                        }

                         _ = req_tui_state.notified() => {
                            let network_tui_state = (&p2p).into();
                            let client_tui_state = (&run).into();
                            tx.send((client_tui_state, network_tui_state))?;
                        },

                        state = watcher.poll_next() => {
                            let (old_state, new_state) = state?;
                            if old_state.map(|s| s.run_state) != Some(new_state.run_state) && new_state.run_state == RunState::RoundTrain {
                                for blob in p2p.currently_sharing_blobs().clone() {
                                    p2p.remove_downloadable(blob).await?;
                                }
                                let p2p_info = get_p2p_info(&p2p).await?;
                                run.set_node_info(p2p_info);
                            }
                            run.apply_state(new_state.clone()).await?;
                        }

                        res = p2p.poll_next() => {
                            if let Some(message) = res? {
                                match message {
                                    NetworkEvent::MessageReceived((from, training_result)) => {
                                        trace!("Got gossip message from {from}: step {} batch id {}", training_result.step, training_result.batch_id);
                                        if let Some(client) = watcher.get_client_for_p2p_public_key(from.as_bytes()) {
                                            run.apply_message(client.id.clone(), training_result).await?;
                                        } else {
                                            warn!("Got broadcast from unknown client {}", from);
                                        }
                                    }
                                    NetworkEvent::DownloadComplete(DownloadComplete {
                                        data: distro_result, hash, ..
                                    }) => {
                                        trace!("Download complete: step {} batch id {}", distro_result.step, distro_result.batch_id);
                                        run.apply_distro_result(hash, distro_result).await;
                                    }
                                    NetworkEvent::DownloadFailed(dl) => {
                                        let retries = *retried_downloads.get(&dl.blob_ticket.hash()).unwrap_or(&0);
                                        if retries >= MAX_DOWNLOAD_RETRIES {
                                            warn!("Download failed (not retrying): {}", dl.error);
                                        } else {
                                            info!("Download failed (retrying): {}", dl.error);
                                            retried_downloads.insert(dl.blob_ticket.hash(), retries + 1);
                                            p2p.start_download(dl.blob_ticket).await?;
                                        }
                                    }
                                }
                            }
                        }

                        () = run.opportunistic_witness_wait_notified() => {
                            run.try_send_opportunistic_witness().await?;
                        }

                        Some(DistroBroadcastAndPayload{ step, batch_id, commitment, proof, distro_result }) = rx_distro_result.recv() => {
                            let ticket = p2p.add_downloadable(distro_result.clone()).await?;
                            let hash = ticket.hash();
                            debug!(
                                "Broadcasting payload step {step} batch id {batch_id} hash 0x{}",
                                hex::encode(hash),
                            );

                            let training_result = TrainingResult { step, batch_id, commitment, ticket, proof };

                            p2p.broadcast(&training_result).await?;

                            // simulate us recving it & apply like anyone else's
                            {
                                run.apply_message(
                                    identity.clone(), training_result
                                ).await?;

                                run.apply_distro_result(hash, distro_result).await;
                            }
                        }

                        Some(download_ticket) = rx_request_download.recv() => {
                            p2p.start_download(download_ticket).await?;
                        }
                        Some(witness) = rx_witness.recv() => {
                            watcher.backend_mut().send_witness(witness).await?;
                        }
                        Some(witness) = rx_health_check.recv() => {
                            watcher.backend_mut().send_health_check(witness).await?;
                        }
                        Some(witness) = rx_checkpoint.recv() => {
                            watcher.backend_mut().send_checkpoint(witness).await?;
                        }
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

    pub fn finished(&mut self) -> &mut JoinHandle<Result<(), Error>> {
        &mut self.join
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub async fn tui_states(&self) -> TUIStates {
        self.req_tui_state.notify_one();
        self.rx.borrow().clone()
    }
}

async fn get_p2p_info<B, D>(
    p2p: &NetworkConnection<B, D>,
) -> anyhow::Result<HashMap<String, DataValue>>
where
    B: Networkable,
    D: Networkable,
{
    let remotes = p2p.remote_infos().await?;
    let node_addr = p2p.node_addr().await?;
    Ok(remotes
        .into_iter()
        .map(|(x, bandwidth)| {
            (
                x.node_id.to_string(),
                HashMap::from([
                    (
                        "ips",
                        DataValue::from(
                            x.addrs
                                .into_iter()
                                .map(|y| y.addr.to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                    ),
                    ("bandwidth", DataValue::from(bandwidth)),
                ])
                .into(),
            )
        })
        .chain(std::iter::once((
            node_addr.node_id.to_string(),
            HashMap::from([
                (
                    "ips",
                    DataValue::from(
                        node_addr
                            .info
                            .direct_addresses
                            .iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ),
                ("bandwidth", DataValue::from(0f32)),
            ])
            .into(),
        )))
        .collect())
}
