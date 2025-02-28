use crate::{
    fetch_data::{BatchIdSet, DataFetcher, TrainingDataForStep},
    state::types::PayloadState,
};

use futures::{future::try_join_all, stream::FuturesUnordered, StreamExt};
use psyche_coordinator::{
    assign_data_for_state, get_batch_ids_for_round, Commitment, Committee, CommitteeSelection,
    Coordinator, CoordinatorError, HealthChecks, RunState, BLOOM_FALSE_RATE,
};
use psyche_core::{sha256, BatchId, Bloom, NodeIdentity};
use psyche_modeling::{
    ApplyDistroResultError, DistroResult, TrainOutput, Trainer, TrainerThreadCommunicationError,
};
use psyche_network::{
    distro_results_to_bytes, AuthenticatableIdentity, SerializeDistroResultError,
    SerializedDistroResult, TransmittableDistroResult,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, debug_span, error, info, trace, warn, Instrument};

use super::{
    evals::{EvalRunner, MaybeRunningEvals},
    round_state::RoundState,
    types::DistroBroadcastAndPayload,
};

#[derive(Debug)]
pub struct FinishedTrainers {
    pub evals_or_trainers: MaybeRunningEvals,
    pub round_losses: Vec<f32>,
    pub optim_stats: HashMap<String, f64>,
    pub round_duration: Duration,
}

#[derive(Error, Debug)]
pub enum TrainError {
    #[error("No trainers available when entering training step.")]
    NoTrainers,

    #[error("No training round in-progress")]
    NoActiveRound,

    #[error("No committee info for this round ")]
    NoCommitteeInfo,

    #[error("We're not in this round")]
    NotInThisRound,

    #[error("Apply thread crashed")]
    ApplyCrashed,

    #[error("Failed to apply distro results: {0}")]
    Apply(#[from] ApplyError),

    #[error("Training thread crashed")]
    TrainCrashed,

    #[error("Failed to train on batch: {0}")]
    TrainOnBatch(#[from] TrainerThreadCommunicationError),

    #[error("Failed to serialize distro result: {0}")]
    SerializeDistroResult(SerializeDistroResultError),

    #[error("Failed to send distro result, channel must be closed")]
    SendDistroResult,

    #[error("Failed to send health checks, channel must be closed")]
    SendHealthChecks,

    #[error("Healthcheck thread crashed")]
    HealthCheckCrashed,

    #[error("Coordinator error: {0}")]
    CoordinatorError(CoordinatorError),
}

pub struct TrainingStepMetadata<T: NodeIdentity, A: AuthenticatableIdentity> {
    pub identity: T,
    pub data_fetcher: DataFetcher<T, A>,
    pub tx_health_check: mpsc::UnboundedSender<HealthChecks>,
    pub tx_distro_result: mpsc::UnboundedSender<DistroBroadcastAndPayload>,

    pub write_gradients_dir: Option<PathBuf>,

    pub eval_runner: EvalRunner,
}

#[derive(Debug)]
pub struct TrainingStep {
    sending_health_checks: Option<JoinHandle<Result<(), TrainError>>>,
    batch_ids_not_yet_trained_on: Arc<Mutex<BatchIdSet>>,
    cancel_training: CancellationToken,

    applying_and_training: JoinHandle<Result<FinishedTrainers, TrainError>>,
}

impl TrainingStep {
    pub async fn finish(self) -> Result<FinishedTrainers, TrainError> {
        self.cancel_training.cancel();
        if let Some(hc) = self.sending_health_checks {
            hc.await.map_err(|_| TrainError::HealthCheckCrashed)??;
        }

        let trainers = self
            .applying_and_training
            .await
            .map_err(|_| TrainError::TrainCrashed)??;

        let remaining_batch_ids = self.batch_ids_not_yet_trained_on.lock().await;
        if !remaining_batch_ids.is_empty() {
            warn!("Training didn't finish when the Training round ended, we are likely to desync. Batch IDs left: {:?}: ", remaining_batch_ids);
        }

        Ok(trainers)
    }
}

impl<T: NodeIdentity, A: AuthenticatableIdentity + 'static> TrainingStepMetadata<T, A> {
    pub fn start(
        &mut self,
        client_index: u64,
        state: &Coordinator<T>,
        trainers: Vec<Trainer>,
        previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> Result<TrainingStep, TrainError> {
        if trainers.is_empty() {
            return Err(TrainError::NoTrainers);
        }

        let applying = self.apply_results(trainers, state, previous_round, current_round)?;

        let round = state.current_round().ok_or(TrainError::NoActiveRound)?;

        let cancel_training = CancellationToken::new();

        let sending_health_checks =
            start_sending_health_checks(current_round, state, self.tx_health_check.clone())?;

        debug!("Transitioning to train step {}", state.progress.step);

        let round_start = Instant::now();

        *previous_round = std::mem::take(current_round);

        let committee_selection = CommitteeSelection::new(
            round.tie_breaker_tasks as usize,
            state.config.witness_nodes as usize,
            state.config.verification_percent,
            state.epoch_state.clients.len(),
            round.random_seed,
        )
        .map_err(TrainError::CoordinatorError)?;

        let data_assignments = assign_data_for_state(state, false, &committee_selection);

        let TrainingDataForStep {
            step,
            num_all_batch_ids: num_batch_ids_for_this_round,
            mut next_sample,
            batch_ids_not_yet_trained_on,
        } = self
            .data_fetcher
            .fetch_data(state, &data_assignments, &self.identity);

        let committee_proof = committee_selection.get_committee(client_index);
        let witness_proof = committee_selection.get_witness(client_index);

        let blooms = match witness_proof.witness {
            true => {
                let participant_bloom =
                    Bloom::random(state.epoch_state.clients.len(), BLOOM_FALSE_RATE);
                let order_bloom = Bloom::random(num_batch_ids_for_this_round, BLOOM_FALSE_RATE);
                debug!(
                    "Participant bloom size: {} bits, {} keys",
                    participant_bloom.bits.0.len(),
                    participant_bloom.keys.len()
                );
                debug!(
                    "Order bloom size: {} bits, {} keys",
                    order_bloom.bits.0.len(),
                    order_bloom.keys.len()
                );
                Some((participant_bloom, order_bloom))
            }
            false => None,
        };

        *current_round = RoundState {
            height: round.height,
            sent_witness: false,
            downloads: Default::default(),
            results: Default::default(),
            commitments_per_client: Default::default(),
            data_assignments,
            blooms,
            committee_info: Some((committee_proof, witness_proof, committee_selection)),
            batch_ids_not_yet_trained_on: Some((
                num_batch_ids_for_this_round,
                batch_ids_not_yet_trained_on.clone(),
            )),
        };

        info!(
            "Assignment for step {} (round {}/epoch {}): index={} committee position={} committee={} witness position={} witness={}",
            state.progress.step, round.height, state.progress.epoch, client_index, committee_proof.position, committee_proof.committee, witness_proof.position, witness_proof.witness
        );

        let eval_runner = self.eval_runner.clone();

        let applying_and_training: JoinHandle<Result<FinishedTrainers, TrainError>> = {
            let identity = self.identity;
            let cancel_training = cancel_training.clone();
            let write_gradients_dir = self.write_gradients_dir.clone();
            let tx_distro_result = self.tx_distro_result.clone();

            tokio::task::spawn(async move {
                let mut round_losses: Vec<f32> = Vec::new();
                let mut optim_stats: HashMap<String, f64> = HashMap::new();

                let mut available_trainers =
                    applying.await.map_err(|_| TrainError::ApplyCrashed)??;

                let mut in_progress = FuturesUnordered::new();

                loop {
                    if !cancel_training.is_cancelled() {
                        // as long as we have any trainer
                        while let Some(trainer) = available_trainers.pop() {
                            // try to give it some data!
                            match next_sample.recv().await {
                                Some(data) => {
                                    let cancel_training = cancel_training.clone();
                                    in_progress.push(tokio::task::spawn_blocking(move || {
                                        trainer.train(step, data, Vec::new(), cancel_training)
                                    }));
                                }
                                // but if we're out of data, then put it back, and don't try to assign anymore trainers.
                                None => {
                                    available_trainers.push(trainer);
                                    break;
                                }
                            }
                        }
                    }

                    // If no tasks are in progress and no trainers are available, we're done.
                    // If training was cancelled, we will eventually end up with all trainers here.
                    if in_progress.is_empty() {
                        debug!(
                            "in_progress is empty, we have {} trainers",
                            available_trainers.len()
                        );
                        break;
                    }

                    // Wait for any training task to complete
                    if let Some(completed_trainer) = in_progress.next().await {
                        let TrainOutput {
                            batch_id,
                            trainer,
                            loss,
                            step,
                            distro_results,
                            cancelled,
                        } = completed_trainer.map_err(|_| TrainError::TrainCrashed)??;

                        let res: Result<(), TrainError> = async {

                            available_trainers.push(trainer);

                            if cancelled {
                                trace!("However, we were cancelled, so we're throwing away this result.");
                                // we're throwing away this result.
                                return Ok(());
                            }

                            let distro_results = distro_results.unwrap_or_default();

                            let distro_result = TransmittableDistroResult {
                                step,
                                batch_id,
                                distro_results: distro_results
                                    .iter()
                                    .map(SerializedDistroResult::try_from)
                                    .collect::<std::result::Result<Vec<_>, _>>()
                                    .map_err(TrainError::SerializeDistroResult)?,
                            };

                            if let Some(dir) = &write_gradients_dir {
                                let distro_result = distro_result.clone();
                                let dir = dir.clone();
                                tokio::spawn(async move {
                                    if let Err(e) =
                                        write_gradients_to_disk(dir, identity, distro_result).await
                                    {
                                        error!("Failed to write gradients to disk: {e}");
                                    }
                                });
                            }

                            for result in distro_results {
                                if let Some(stats) = result.stats {
                                    for (name, value) in stats {
                                        // a rolling average for this step :)
                                        optim_stats
                                            .entry(name)
                                            .and_modify(|e| *e = (*e + value) / 2.0)
                                            .or_insert(value);
                                    }
                                }
                            }

                            round_losses.push(loss);

                            let mut committment = Vec::with_capacity(40);
                            committment.extend_from_slice(identity.as_ref());
                            committment.extend_from_slice(&u64::from(batch_id).to_be_bytes());
                            let commitment = sha256(&committment);

                            trace!("trying to queue tx distro result...");
                            tx_distro_result
                                .send(DistroBroadcastAndPayload {
                                    step,
                                    batch_id,
                                    commitment,
                                    proof: committee_proof,
                                    distro_result,
                                })
                                .map_err(|_| TrainError::SendDistroResult)?;
                            trace!("successfully queued tx distro result");
                            Ok(())
                        }.instrument(
                            tracing::debug_span!(
                                "train_done",
                                batch_id = format!("{batch_id}")
                            )
                        ).await;
                        res?;
                    }
                }

                let evals = if cancel_training.is_cancelled() {
                    // we got timed out, don't bother starting evals
                    MaybeRunningEvals::NotRunning(available_trainers)
                } else {
                    // we finished before getting cancelled, have some time to start evals.
                    MaybeRunningEvals::Running(eval_runner.start(available_trainers))
                };
                let round_duration = Instant::now() - round_start;
                debug!("Training for round finished, duration {:?}", round_duration);
                Ok(FinishedTrainers {
                    evals_or_trainers: evals,
                    round_losses,
                    optim_stats,
                    round_duration,
                })
            })
        };

        Ok(TrainingStep {
            applying_and_training,
            cancel_training,
            sending_health_checks,
            batch_ids_not_yet_trained_on,
        })
    }

    fn apply_results(
        &mut self,
        trainers: Vec<Trainer>,
        state: &Coordinator<T>,
        _previous_round: &mut RoundState<T>,
        current_round: &mut RoundState<T>,
    ) -> Result<JoinHandle<Result<Vec<Trainer>, ApplyError>>, ApplyError> {
        if state.epoch_state.first_round.into() {
            // the first training step of each epoch has no apply phase.
            info!("Skipping early apply");
            return Ok(tokio::task::spawn(async move { Ok(trainers) }));
        }

        // coordinator has already advanced to the next round (unless we're in cooldown) but we haven't started ours yet.
        // so our current_round corresponds to the coordinator's previous_round
        let round = match state.run_state == RunState::Cooldown {
            false => state.previous_round(),
            true => state.current_round(),
        }
        .ok_or(ApplyError::NoActiveRound)?;

        let apply_start = Instant::now();
        let step = state.progress.step;
        let witness_quorum = state.config.witness_quorum;

        let round_to_take_from = current_round;

        let mut payloads = std::mem::take(&mut round_to_take_from.downloads);
        let commitments = std::mem::take(&mut round_to_take_from.results);

        assert!(!payloads.is_empty());
        assert!(!commitments.is_empty());

        let witnesses = round.witnesses;
        let batch_ids = get_batch_ids_for_round(
            // coordinator has already advanced to the next round but we haven't started ours yet.
            // our current_round corresponds to the coordinator's previous_round
            state.previous_round().ok_or(ApplyError::NoActiveRound)?,
            state,
        );

        let b = batch_ids.clone();
        Ok(tokio::task::spawn(async move {
                let mut distro_results: Vec<Vec<DistroResult>> = Vec::new();

                for batch_id in batch_ids {
                    let batch_commitments = match commitments.get(&batch_id) {
                        Some(x) => x,
                        None => {
                            warn!("No commitments for batch {batch_id}");
                            continue;
                        }
                    };
                    debug!("Commitments for batch {batch_id}: {batch_commitments:?}");
                    let consensus = match Coordinator::<T>::select_consensus_commitment_by_witnesses(
                        &batch_commitments
                            .iter()
                            .map(|x| x.1.commitment)
                            .collect::<Vec<_>>(),
                        &witnesses,
                        witness_quorum,
                    ) {
                        Some(x) => x,
                        None => {
                            warn!("No consensus commitment for batch {}", batch_id);
                            continue;
                        }
                    };
                    debug!("Consensus commitment for batch {batch_id}: {consensus:?}");

                    let consensus = &batch_commitments[consensus].1;
                    let maybe_results = match payloads.remove(&consensus.ticket.hash()) {
                        Some(PayloadState::Deserializing(x)) => match x.is_finished() {
                            true => x.await.unwrap(),
                            false => {
                                return Err(ApplyError::DidNotFinishDeserializingCommitment(
                                    consensus.commitment,
                                    batch_id,
                                ));
                            }
                        },
                        Some(PayloadState::Downloading(_)) => {
                            return Err(ApplyError::DidNotBeginDownloadingCommitment(
                                consensus.commitment,
                                batch_id,
                            ));
                        }
                        None => {
                            return Err(ApplyError::UnknownCommitment(
                                consensus.commitment,
                                batch_id,
                            ))
                        }
                    };

                    match maybe_results {
                        Ok(results) => {
                            distro_results.push(results);
                        }
                        Err(err) => warn!("DESYNC: Got the following error when deserializing results for commitment 0x{}: {}", hex::encode(consensus.commitment), err),
                    }
                }

                let futures: Vec<JoinHandle<std::result::Result<Trainer, ApplyDistroResultError>>> =
                    trainers
                        .into_iter()
                        .map(|trainer| {
                            let distro_results = Some(distro_results.clone());

                            tokio::task::spawn_blocking(move || {
                                trainer.optimize(step, distro_results)
                            })
                        })
                        .collect::<Vec<_>>();
                let trainers: Vec<_> = try_join_all(futures)
                    .await
                    .map_err(|_| ApplyDistroResultError::ThreadCrashed)?
                    .into_iter()
                    .collect::<Result<_, _>>()?;
                debug!(
                    "Apply time: {:.1}s, {} trainers ready",
                    (Instant::now() - apply_start).as_secs_f32(),
                    trainers.len()
                );
                Ok(trainers)
            }.instrument(debug_span!("Applying distro results", ?b))))
    }
}

fn start_sending_health_checks<T: NodeIdentity>(
    current_round: &mut RoundState<T>,
    state: &Coordinator<T>,
    tx_health_check: mpsc::UnboundedSender<HealthChecks>,
) -> Result<Option<JoinHandle<Result<(), TrainError>>>, TrainError> {
    // we won't have any information to health check with until at least one round of training has finished
    if state.epoch_state.first_round.into() {
        return Ok(None);
    }
    let (_, witness_proof, committee_selection) = current_round
        .committee_info
        .as_ref()
        .ok_or(TrainError::NoCommitteeInfo)?;
    Ok(
        if state.epoch_state.first_round.is_false() && witness_proof.witness {
            let witnesses = state
                .previous_round()
                .ok_or(TrainError::NoActiveRound)?
                .witnesses;
            let witness_quorum = state.config.witness_quorum;
            let clients = state.epoch_state.clients;
            let committee_selection = committee_selection.clone();
            let state = *state;
            Some(tokio::task::spawn(async move {
                let mut checks = HealthChecks::new();
                for (index, client) in clients.iter().enumerate() {
                    let proof = committee_selection.get_committee(index as u64);
                    if proof.committee == Committee::Trainer
                        && !Coordinator::trainer_healthy_by_witnesses(
                            &state,
                            &client.id,
                            &witnesses,
                            witness_quorum,
                        )
                    {
                        warn!(
                            unhealthy_warn = "Found unhealthy trainer at",
                            index = index,
                            client_id = %&client.id,
                        );
                        checks.push(proof);
                    }
                }

                if !checks.is_empty() {
                    info!("Sending health check for following indicies: {:?}", checks);
                    tx_health_check
                        .send(checks)
                        .map_err(|_| TrainError::SendHealthChecks)
                } else {
                    Ok(())
                }
            }))
        } else {
            None
        },
    )
}

#[derive(Error, Debug)]
pub enum ApplyError {
    #[error("no active round")]
    NoActiveRound,

    #[error("failed to apply distro result: {0}")]
    BadResult(#[from] ApplyDistroResultError),

    #[error("DESYNC: Did not finish deserializing payload for consensus commitment 0x{commitment} for batch {1}", commitment=hex::encode(.0))]
    DidNotFinishDeserializingCommitment(Commitment, BatchId),

    #[error("DESYNC: Did not begin downloading payload for consensus commitment 0x{commitment} for batch {1}", commitment=hex::encode(.0))]
    DidNotBeginDownloadingCommitment(Commitment, BatchId),

    #[error("DESYNC: Unknown consensus commitment 0x{commitment} for batch {1}", commitment=hex::encode(.0))]
    UnknownCommitment(Commitment, BatchId),
}

#[derive(Debug, Error)]
enum WriteGradientsError {
    #[error("Failed to create write_gradients_dir: {0}")]
    CreateDir(tokio::io::Error),

    #[error("Failed to serialize distro result data {fname} to bytes: {err}")]
    Serialize { fname: String, err: postcard::Error },

    #[error("Failed to write distro result data {fname}: {err}")]
    Write {
        fname: String,
        err: tokio::io::Error,
    },
}

async fn write_gradients_to_disk<T: NodeIdentity>(
    write_gradients_dir: PathBuf,
    identity: T,
    distro_result: TransmittableDistroResult,
) -> Result<(), WriteGradientsError> {
    debug!("Trying to write distro result to disk...");
    tokio::fs::create_dir_all(&write_gradients_dir)
        .await
        .map_err(WriteGradientsError::CreateDir)?;

    let fname = format!(
        "result-{}-step{}-batch{}.vec-postcard",
        identity, distro_result.step, distro_result.batch_id
    );
    let fpath = write_gradients_dir.join(&fname);
    let serialized = distro_results_to_bytes(&distro_result.distro_results).map_err(|err| {
        WriteGradientsError::Serialize {
            fname: fname.clone(),
            err,
        }
    })?;
    tokio::fs::write(fpath, serialized)
        .await
        .map_err(|err| WriteGradientsError::Write {
            fname: fname.clone(),
            err,
        })?;
    debug!("Wrote distro result {fname}.");
    Ok(())
}
