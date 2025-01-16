use crate::{
    state::{DistroBroadcastAndPayload, RunManager},
    ClientTUIState, RunInitConfig, RunInitConfigAndIO, TrainingResult, NC,
};
use anyhow::{Error, Result};
use psyche_coordinator::RunState;
use psyche_core::NodeIdentity;
use psyche_network::{
    allowlist, request_model_parameter, AuthenticatableIdentity, DownloadComplete, ModelParameters,
    NetworkConnection, NetworkEvent, NetworkTUIState, Networkable, NodeId, TransmittableDownload,
};
use psyche_watcher::{Backend, BackendWatcher};
use wandb::DataValue;

use std::{collections::HashMap, marker::PhantomData, sync::Arc};
use tokio::{
    select,
    sync::{mpsc, watch, Notify},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
pub type TUIStates = (ClientTUIState, NetworkTUIState);

pub struct Client<T: NodeIdentity, A: AuthenticatableIdentity, B: Backend<T> + 'static> {
    rx_tui: watch::Receiver<TUIStates>,
    req_tui_state: Arc<Notify>,
    cancel: CancellationToken,
    join: JoinHandle<Result<()>>,
    _t: PhantomData<(T, A, B)>,
}

const MAX_DOWNLOAD_RETRIES: usize = 3;

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static, B: Backend<T> + 'static>
    Client<T, A, B>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: B,
        allowlist: allowlist::AllowDynamic,
        mut p2p: NC,
        init_config: RunInitConfig<T, A>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let (tx_tui, rx_tui) = watch::channel::<TUIStates>(Default::default());
        let req_tui_state = Arc::new(Notify::new());

        let identity = init_config.identity;
        let join = tokio::spawn({
            let cancel = cancel.clone();
            let req_tui_state = req_tui_state.clone();
            async move {
                let mut watcher = BackendWatcher::new(backend);

                // From Run
                let (tx_witness, mut rx_witness) = mpsc::unbounded_channel();
                let (tx_health_check, mut rx_health_check) = mpsc::unbounded_channel();
                let (tx_checkpoint, mut rx_checkpoint) = mpsc::unbounded_channel();
                let (tx_model, mut rx_model) = mpsc::unbounded_channel();
                let (tx_distro_result, mut rx_distro_result) = mpsc::unbounded_channel();
                let (tx_request_download, mut rx_request_download) = mpsc::unbounded_channel();
                let (tx_parameters_req, mut rx_parameters_req) = mpsc::unbounded_channel();
                let (tx_params_download, mut rx_params_download) = mpsc::unbounded_channel();

                let mut run = RunManager::<T, A>::new(RunInitConfigAndIO {
                    init_config,

                    tx_witness,
                    tx_health_check,
                    tx_checkpoint,
                    tx_model,
                    tx_parameters_req,
                    tx_distro_result,
                    tx_request_download,
                });

                let mut retried_downloads: HashMap<psyche_network::Hash, usize> = HashMap::new();
                let mut sharable_model = ModelParameters::empty();
                loop {
                    select! {
                        _ = cancel.cancelled() => {
                            break;
                        }

                         _ = req_tui_state.notified() => {
                            let network_tui_state = (&p2p).into();
                            let client_tui_state = (&run).into();
                            tx_tui.send((client_tui_state, network_tui_state))?;
                        },

                        state = watcher.poll_next() => {
                            let (old_state, new_state) = state?;
                            {
                                let node_ids: Vec<NodeId> = new_state
                                    .epoch_state
                                    .clients
                                    .iter()
                                    .map(|c| NodeId::from_bytes(c.id.get_p2p_public_key()).unwrap()).collect();
                                if node_ids.contains(&p2p.node_id()) {
                                    // only connect to peers after we become part of the set of current clients
                                    p2p.add_peers(node_ids.clone()).await?;
                                }
                                allowlist.set(node_ids);
                            }

                            if old_state.map(|s| s.run_state) != Some(new_state.run_state) && new_state.run_state == RunState::RoundTrain {
                                for blob in p2p.currently_sharing_blobs().clone() {
                                    p2p.remove_downloadable(blob).await?;
                                }
                                let p2p_info = get_p2p_info(&p2p).await?;
                                run.set_node_info(p2p_info);
                            }
                            run.apply_state(*new_state).await?;
                        }

                        res = p2p.poll_next() => {
                            if let Some(message) = res? {
                                match message {
                                    NetworkEvent::MessageReceived((from, training_result)) => {
                                        trace!("Got gossip message from {from}: step {} batch id {}", training_result.step, training_result.batch_id);
                                        if let Some(client) = watcher.get_client_for_p2p_public_key(from.as_bytes()) {
                                            run.apply_message(client.id, training_result).await?;
                                        } else {
                                            warn!("Got broadcast from unknown client {}", from);
                                        }
                                    }
                                    NetworkEvent::DownloadComplete(DownloadComplete {
                                        data: download_data, hash, ..
                                    }) => {
                                        match download_data {
                                            TransmittableDownload::DistroResult(distro_result) => {
                                                trace!("Download complete: step {} batch id {}", distro_result.step, distro_result.batch_id);
                                                run.apply_distro_result(hash, distro_result).await;
                                            },
                                            TransmittableDownload::ModelParameter(parameter) => {
                                                sharable_model.add_parameter(parameter)?;
                                                if sharable_model.is_download_complete() {
                                                    sharable_model.send_init_parameters()?;
                                                }
                                            },
                                        }
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
                                    NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx) => {

                                        // TODO: We should validate things here:
                                        //  * Make sure that the parameter is requested while we are in RunState::Warmup.
                                        //  * Validate that the message is from a known peer.

                                        let transmittable_parameter = sharable_model.get_transmittable_parameter(&parameter_name);
                                        if let Err(e) = transmittable_parameter {
                                            if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                            }
                                        } else {
                                            let transmittable_download = TransmittableDownload::ModelParameter(transmittable_parameter.unwrap());
                                            let ticket = p2p.add_downloadable(transmittable_download).await?;

                                            // TODO: Here we should probably encode & sign beforehand, and then pass it to the protocol to respond
                                            // to the client

                                            info!("Sending requested model parameter blob ticket");
                                            if let Err(e) = protocol_req_tx.send(Ok(ticket)) {
                                                warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                            };
                                        }
                                    }
                                }
                            }
                        }

                        () = run.opportunistic_witness_wait_notified() => {
                            run.try_send_opportunistic_witness().await?;
                        }

                        Some(DistroBroadcastAndPayload{ step, batch_id, commitment, proof, distro_result }) = rx_distro_result.recv() => {
                            let transmittable_distro_result = TransmittableDownload::DistroResult(distro_result.clone());
                            let ticket = p2p.add_downloadable(transmittable_distro_result).await?;
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
                                    identity, training_result
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
                        Some(model) = rx_model.recv() => {
                            sharable_model.update_parameters(model)?;
                        },
                        Some((param_names, tx_params_response)) = rx_parameters_req.recv() => {
                            sharable_model.initialize_parameters(&param_names, tx_params_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                warn!("Coordinator state not yet registered, nothing to do");
                                return Ok(());
                            };

                            let tx_params_download = tx_params_download.clone();
                            let router = p2p.router();

                            let me = NodeId::from_bytes(identity.get_p2p_public_key()).unwrap();
                            let peer_ids: Vec<NodeId> = coordinator_state.epoch_state.clients.iter().map(|client| {
                                let peer_id_bytes = client.id.get_p2p_public_key();
                                NodeId::from_bytes(peer_id_bytes).unwrap()
                            })
                            .filter(|peer_id| peer_id != &me)
                            .collect();

                            let handle: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
                                let mut parameter_blob_tickets = Vec::new();
                                // TODO: The parameter requests could be done concurrently, setting some MAX_CONCURRENT_PARAM_REQUESTS

                                let mut peer_iter = peer_ids.iter().cycle(); // Iterate over peers in a cycle
                                for param_name in param_names {
                                    loop {
                                        if let Some(peer_id) = peer_iter.next() {
                                            let router = router.clone();
                                            debug!("Requesting parameter {param_name} from peer {peer_id}");
                                            match request_model_parameter(router, *peer_id, param_name.clone()).await {
                                                Ok(parameter_blob_ticket) => {
                                                    parameter_blob_tickets.push(parameter_blob_ticket);
                                                    // Continue to the next parameter
                                                    break;
                                                }
                                                Err(e) => {
                                                    warn!("Failed to get parameter {param_name} from peer {peer_id}: {e}");
                                                    // Continue to the next peer
                                                }
                                            }
                                        }
                                    }
                                }
                                tx_params_download.send(parameter_blob_tickets)?;
                                Ok(())
                            });
                            drop(handle);
                        }
                        Some(param_blob_tickets) = rx_params_download.recv() => {
                            for ticket in param_blob_tickets {
                                p2p.start_download(ticket).await?;
                            }
                        }
                        else => break
                    }
                }
                Ok(())
            }
        });

        Self {
            _t: Default::default(),
            cancel,
            req_tui_state,
            rx_tui,
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
        self.rx_tui.borrow().clone()
    }
}

async fn get_p2p_info<B, D>(
    p2p: &NetworkConnection<B, D>,
) -> anyhow::Result<HashMap<String, DataValue>>
where
    B: Networkable,
    D: Networkable,
{
    let remotes = p2p.remote_infos();
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
