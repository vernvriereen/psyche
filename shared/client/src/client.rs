use crate::{
    state::{DistroBroadcastAndPayload, RunManager},
    ClientTUIState, RunInitConfig, RunInitConfigAndIO, TrainingResult, NC,
};
use anyhow::{bail, Error, Result};
use futures::future::join_all;
use psyche_coordinator::RunState;
use psyche_core::NodeIdentity;
use psyche_network::{
    allowlist, request_model, AuthenticatableIdentity, BlobTicket, DownloadComplete,
    ModelRequestType, NetworkConnection, NetworkEvent, NetworkTUIState, Networkable, NodeId,
    SharableModel, TransmittableDownload,
};
use psyche_watcher::{Backend, BackendWatcher};
use tokenizers::Tokenizer;
use wandb::DataValue;

use rand::{seq::SliceRandom, thread_rng};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
};
use tokio::{
    select,
    sync::{mpsc, watch, Mutex, Notify},
    task::JoinHandle,
    time::interval,
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
const REBROADCAST_SHAREABLE: Duration = Duration::from_secs(2);

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
                #[cfg(not(feature = "parallelism"))]
                if init_config.tensor_parallelism != 1 {
                    anyhow::bail!("Tensor parallelism was set but this build does not support it (must be built with --features=parallelism)")
                }

                let mut watcher = BackendWatcher::new(backend);

                // From Run
                let (tx_witness, mut rx_witness) = mpsc::unbounded_channel();
                let (tx_health_check, mut rx_health_check) = mpsc::unbounded_channel();
                let (tx_checkpoint, mut rx_checkpoint) = mpsc::unbounded_channel();
                let (tx_model, mut rx_model) = mpsc::unbounded_channel();
                let (tx_distro_result, mut rx_distro_result) = mpsc::unbounded_channel();
                let (tx_request_download, mut rx_request_download) = mpsc::unbounded_channel();
                let (tx_parameters_req, mut rx_parameters_req) = mpsc::unbounded_channel();
                let (tx_config, mut rx_config) = mpsc::unbounded_channel();
                let (tx_params_download, mut rx_params_download) = mpsc::unbounded_channel();
                let (tx_request_model_config, mut rx_request_model_config) =
                    mpsc::unbounded_channel();

                let max_concurrent_downloads = init_config.max_concurrent_parameter_requests;

                let mut run = RunManager::<T, A>::new(RunInitConfigAndIO {
                    init_config,

                    tx_witness,
                    tx_health_check,
                    tx_checkpoint,
                    tx_model,
                    tx_parameters_req,
                    tx_config,
                    tx_distro_result,
                    tx_request_download,
                    tx_request_model_config,
                });

                let mut retried_downloads: HashMap<psyche_network::Hash, usize> = HashMap::new();
                let mut sharable_model = SharableModel::empty();
                let mut sharing_downloadables = vec![];
                let mut sharing_downloadables_rebroadcast_index = 0;
                let mut sharing_downloadable_interval = interval(REBROADCAST_SHAREABLE);
                debug!("Starting client loop");
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
                            let old_run_state = old_state
                                .map(|s| s.run_state.to_string())
                                .unwrap_or_else(|| String::from(" - "));

                            trace!(
                                client_id = %identity,
                                old_state = old_run_state,
                                new_state = new_state.run_state.to_string(),
                                "apply_state"
                            );

                            let peer_node_ids = p2p.get_all_peers().await.0.into_iter().map(|x| x.node_id).collect::<BTreeSet<_>>();
                            {
                                let node_ids: Vec<NodeId> = new_state
                                    .epoch_state
                                    .clients
                                    .iter()
                                    .map(|c| NodeId::from_bytes(c.id.get_p2p_public_key()).unwrap()).collect();
                                if node_ids.contains(&p2p.node_id()) {
                                    // only connect to peers after we become part of the set of current clients
                                    let to_connect = node_ids.iter().filter(|x| !peer_node_ids.contains(*x)).collect::<Vec<_>>();
                                    if !to_connect.is_empty() {
                                        info!(num_new_peers = to_connect.len(), "Connecting to new peers");
                                        p2p.add_peers(node_ids.clone()).await?;
                                    }
                                }
                                allowlist.set(node_ids);
                            }

                            if old_state.map(|s| s.run_state) != Some(new_state.run_state) && new_state.run_state == RunState::RoundTrain {
                                debug!(num_peers = peer_node_ids.len(), "Updating p2p");
                                let last_needed_step_blobs = new_state.progress.step.saturating_sub(2);
                                p2p.remove_blobs_with_tag_less_than(last_needed_step_blobs);
                                let p2p_info = get_p2p_info(&p2p).await?;
                                run.set_node_info(p2p_info);
                                sharing_downloadables.retain(|(_, step)| *step >= last_needed_step_blobs);
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
                                                run.apply_distro_result(hash, distro_result, None).await;
                                            },
                                            TransmittableDownload::ModelParameter(parameter) => {
                                                sharable_model.add_parameter(parameter)?;
                                                if sharable_model.is_download_complete() {
                                                    sharable_model.send_init_parameters()?;
                                                }
                                            },
                                            TransmittableDownload::ModelConfig(config) => {
                                                sharable_model.add_config(config)?;
                                                sharable_model.send_config()?;
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
                                            p2p.start_download(dl.blob_ticket, dl.tag).await?;
                                        }
                                    }
                                    NetworkEvent::ParameterRequest(parameter_name, protocol_req_tx) => {

                                        // TODO: We should validate that the parameter is requested while we are in RunState::Warmup.

                                        match sharable_model.get_transmittable_parameter(&parameter_name) {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(transmittable_parameter) => {
                                                 let transmittable_download = TransmittableDownload::ModelParameter(transmittable_parameter);
                                            // tag 0 means when we enter a train step, it'll get wiped.
                                            let ticket = p2p.add_downloadable(transmittable_download, 0).await?;

                                            // TODO: Here we should probably encode & sign beforehand, and then pass it to the protocol to respond
                                            // to the client

                                            info!("Sending requested model parameter blob ticket");
                                            if let Err(e) = protocol_req_tx.send(Ok(ticket)) {
                                                warn!("Could not send model parameter {parameter_name} blob ticket. Error: {e:?}");
                                            };
                                            }
                                        }
                                    },
                                    NetworkEvent::ModelConfigRequest(protocol_req_tx) => {
                                        match sharable_model.get_transmittable_config() {
                                            Err(e) => {
                                                if let Err(e) = protocol_req_tx.send(Err(e)) {
                                                    warn!("Could not send model config blob ticket. Error: {e:?}");
                                                }
                                            },
                                            Ok(sharable_config) => {
                                                let transmittable_config = TransmittableDownload::ModelConfig(sharable_config);
                                                // tag 0 means when we enter a train step, it'll get wiped.
                                                let config_ticket = p2p.add_downloadable(transmittable_config, 0).await?;

                                                info!("Sending requested model config blob ticket");
                                                if let Err(e) = protocol_req_tx.send(Ok(config_ticket)) {
                                                    warn!("Could not send model config blob ticket. Error: {e:?}");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        () = run.opportunistic_witness_wait_notified() => {
                            run.try_send_opportunistic_witness().await?;
                        }

                        Some(DistroBroadcastAndPayload{ step, batch_id, commitment, proof, distro_result, original_distro_result }) = rx_distro_result.recv() => {
                            let transmittable_distro_result = TransmittableDownload::DistroResult(distro_result.clone());
                            let ticket = p2p.add_downloadable(transmittable_distro_result, step).await?;
                            let hash = ticket.hash();
                            debug!(
                                "Broadcasting payload step {step} batch id {batch_id} hash 0x{}",
                                hex::encode(hash),
                            );

                            let training_result = TrainingResult { step, batch_id, commitment, ticket, proof, nonce: 0 };

                            p2p.broadcast(&training_result).await?;
                            sharing_downloadables.push((training_result.clone(), step));

                            // simulate us recving it & apply like anyone else's
                            {
                                run.apply_message(
                                    identity, training_result
                                ).await?;

                                // VERY IMPORTANT -- we pass the "original" distro result, which is unquantized
                                // even if quantization is turned on (distro_result is quantized).
                                // this is because distro needs the unquantized version for lookahead
                                run.apply_distro_result(hash, distro_result, Some(original_distro_result)).await;
                            }
                        }

                        _ = sharing_downloadable_interval.tick() => {
                            match sharing_downloadables.len() {
                                0 => {},
                                len => {
                                    // it's possible we've disconnected from a gossip peer, but we don't know until we try and send to them.
                                    // in general, iroh-gossip doesn't guarantee delivery. so, we rebroadcast our live results (-2 rounds)
                                    // periodically
                                    sharing_downloadables_rebroadcast_index = (sharing_downloadables_rebroadcast_index + 1) % len;
                                    let (training_result, step) = &mut sharing_downloadables[sharing_downloadables_rebroadcast_index];
                                    training_result.nonce += 1;
                                    trace!("Rebroadcasting payload step {} batch id {} hash 0x{}", step, training_result.batch_id, hex::encode(training_result.ticket.hash()));
                                    p2p.broadcast(training_result).await?;
                                }
                            }

                        }

                        Some((download_ticket, tag)) = rx_request_download.recv() => {
                            p2p.start_download(download_ticket, tag).await?;
                        }
                        Some(witness) = rx_witness.recv() => {
                            watcher.backend_mut().send_witness(witness).await?;
                        }
                        Some(health_check) = rx_health_check.recv() => {
                            watcher.backend_mut().send_health_check(health_check).await?;
                        }
                        Some(checkpoint) = rx_checkpoint.recv() => {
                            watcher.backend_mut().send_checkpoint(checkpoint).await?;
                        }
                        Some(model) = rx_model.recv() => {
                            sharable_model.update_parameters(model)?;
                        },
                        Some((config_string, tokenizer_string)) = rx_config.recv() => {
                            let tokenizer: Tokenizer = serde_json::from_str(&tokenizer_string)?;
                            sharable_model.update_config(config_string, tokenizer)?;
                        }
                        Some((param_names, tx_params_response)) = rx_parameters_req.recv() => {
                            sharable_model.initialize_parameters(&param_names, tx_params_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                bail!("Coordinator state not yet registered, nothing to do. Try joining the run again.");
                            };

                            let tx_params_download = tx_params_download.clone();
                            let router = p2p.router();

                            let me = NodeId::from_bytes(identity.get_p2p_public_key()).unwrap();
                            let mut peer_ids: Vec<NodeId> = coordinator_state.epoch_state.clients.iter().map(|client| {
                                let peer_id_bytes = client.id.get_p2p_public_key();
                                NodeId::from_bytes(peer_id_bytes).unwrap()
                            })
                            .filter(|peer_id| peer_id != &me)
                            .collect();

                            if peer_ids.is_empty() {
                                bail!("There are no peers to request parameters from. Try joining the run again.");
                            }
                            peer_ids.shuffle(&mut thread_rng());

                            let handle: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
                                // We use std mutex implementation here and call `.unwrap()` when acquiring the lock since there
                                // is no chance of mutex poisoning; locks are acquired only to insert or remove items from them
                                // and dropped immediately
                                let parameter_blob_tickets = Arc::new(std::sync::Mutex::new(Vec::new()));
                                let busy_peers = Arc::new(std::sync::Mutex::new(HashSet::new()));

                                let peer_cycle = peer_ids.into_iter().cycle();
                                let peer_cycle = Arc::new(Mutex::new(peer_cycle));
                                let mut request_handles = Vec::new();

                                for param_name in param_names {
                                    let router = router.clone();
                                    let busy_peers = busy_peers.clone();
                                    let parameter_blob_tickets_clone = parameter_blob_tickets.clone();
                                    let peer_cycle = peer_cycle.clone();

                                    let request_handle = tokio::spawn(async move {
                                        loop {
                                            let Some(peer_id) = peer_cycle.lock().await.next() else {
                                                // This should never really happen, since the only chance for calling
                                                // `next()` on a `Cycle` iterator and return `None` is when the iterator
                                                // is empty, which was checked previously.
                                                unreachable!();
                                            };
                                            if !busy_peers.lock().unwrap().insert(peer_id) {
                                                continue;
                                            }
                                            debug!("Requesting parameter {param_name} from peer {peer_id}");
                                            match request_model(router.clone(), peer_id, ModelRequestType::Parameter(param_name.clone())).await {
                                                Ok(parameter_blob_ticket) => {
                                                  parameter_blob_tickets_clone.lock().unwrap().push(parameter_blob_ticket);
                                                  busy_peers.lock().unwrap().remove(&peer_id);
                                                  // Continue to next parameter request
                                                  break;
                                                },
                                                Err(e) => {
                                                  warn!("Failed to get parameter {param_name} from peer {peer_id}: {e}");
                                                  busy_peers.lock().unwrap().remove(&peer_id);
                                                  // Continue to request this parameter to another peer
                                                  continue;
                                                },
                                            }
                                        }
                                    });

                                    // Check if we reached the max number of concurrent requests, and if that is the case,
                                    // await for all of them to complete and start downloading the blobs
                                    if request_handles.len() == max_concurrent_downloads - 1 {
                                        let mut max_concurrent_request_futures = std::mem::take(&mut request_handles);
                                        max_concurrent_request_futures.push(request_handle);
                                        join_all(max_concurrent_request_futures).await;
                                        let current_parameter_blob_tickets: Vec<BlobTicket> = {
                                            let mut parameter_blob_tickets_lock = parameter_blob_tickets.lock().unwrap();
                                            parameter_blob_tickets_lock.drain(..).collect()
                                        };
                                        tx_params_download.send(current_parameter_blob_tickets)?;
                                        continue;
                                    }
                                    request_handles.push(request_handle);
                                }

                                // All parameters have been requested, wail all the remaining request futures to complete
                                // and download the blobs
                                join_all(request_handles).await;
                                let parameter_blob_tickets: Vec<BlobTicket> = {
                                    let mut parameter_blob_tickets_lock = parameter_blob_tickets.lock().unwrap();
                                    parameter_blob_tickets_lock.drain(..).collect()
                                };
                                tx_params_download.send(parameter_blob_tickets)?;
                                Ok(())
                            });
                            drop(handle);
                        },
                        Some(tx_model_config_response) = rx_request_model_config.recv() => {
                            sharable_model.tx_model_config_response = Some(tx_model_config_response);
                            let Some(coordinator_state) = watcher.coordinator_state() else {
                                warn!("Coordinator state not yet registered, nothing to do");
                                return Ok(());
                            };
                            let router = p2p.router();
                            let me = NodeId::from_bytes(identity.get_p2p_public_key())?;
                            let peer_ids: Vec<NodeId> = coordinator_state.epoch_state.clients.iter().map(|client| {
                                let peer_id_bytes = client.id.get_p2p_public_key();
                                NodeId::from_bytes(peer_id_bytes).unwrap()
                            })
                            .filter(|peer_id| peer_id != &me)
                            .collect();
                            let peer_ids_iter = peer_ids.into_iter().cycle();
                            for peer_id in peer_ids_iter {
                                match request_model(router.clone(), peer_id, ModelRequestType::Config).await {
                                    Ok(ticket) => {
                                        // tag 0 means when we enter a train step, it'll get wiped.
                                        p2p.start_download(ticket, 0).await?;
                                        break;
                                    }
                                    Err(err) => warn!("Error obtaining blob ticket for model config: {}", err),
                                }
                            }
                        }
                        Some(param_blob_tickets) = rx_params_download.recv() => {
                            for ticket in param_blob_tickets {
                                // tag 0 means when we enter a train step, it'll get wiped.
                                p2p.start_download(ticket, 0).await?;
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
