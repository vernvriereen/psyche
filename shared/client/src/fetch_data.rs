use psyche_coordinator::{get_batch_ids_for_round, Coordinator};
use psyche_core::{BatchId, IntervalTree};
use psyche_data_provider::{DataProviderTcpClient, TokenizedDataProvider};
use psyche_network::NetworkableNodeIdentity;
use std::{collections::HashSet, sync::Arc};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tracing::{debug, error, info_span, Instrument};

pub type BatchStep = u32;
pub type BatchIdSet = HashSet<BatchId>;

#[derive(Debug, Clone)]
pub struct Batch {
    pub id: BatchId,
    pub data: Vec<Vec<i32>>,
}

pub struct DataFetcher<T: NetworkableNodeIdentity> {
    data_provider: Arc<Mutex<DataProviderTcpClient<T>>>,
    active_fetch_task: Option<(BatchStep, JoinHandle<()>)>,
    buffer_size: usize,
}

impl<T: NetworkableNodeIdentity> DataFetcher<T> {
    pub fn new(data_provider: DataProviderTcpClient<T>, buffer_size: usize) -> Self {
        Self {
            data_provider: Arc::new(Mutex::new(data_provider)),
            active_fetch_task: None,
            buffer_size,
        }
    }

    pub fn fetch_data(
        &mut self,
        state: &Coordinator<T>,
        data_assignments: &IntervalTree<BatchId, T>,
        identity: &T,
    ) -> TrainingDataForStep {
        let step = state.step;
        let data_indicies_per_batch = state.data_indicies_per_batch;

        let mut assigned_batch_ids: Vec<BatchId> = data_assignments
            .iter()
            .filter_map(|(key, value)| match value == identity {
                true => {
                    let batch_interval = (u64::from(key.start)
                        / state.data_indicies_per_batch as u64)
                        ..=(u64::from(key.end) / state.data_indicies_per_batch as u64);
                    Some(batch_interval)
                }
                false => None,
            })
            .flatten()
            .map(BatchId::from_u64)
            .collect();

        // TODO: replace `get_batch_ids_for_state` with a version that's aware of training/verify/tiebreak (or use assigned_batch_ids).
        let all_batch_ids = get_batch_ids_for_round(state.current_round().unwrap(), state);
        let num_all_batch_ids = all_batch_ids.len();
        debug!("Got new batch IDs for step {step} - there are {num_all_batch_ids}");
        debug!(
            "all assignments:{}\nmy assignments: {:?}\nmy id: {}",
            data_assignments, assigned_batch_ids, identity
        );
        let batch_ids_not_yet_trained_on: std::sync::Arc<Mutex<BatchIdSet>> =
            Arc::new(Mutex::new(all_batch_ids.into_iter().collect()));

        let (tx_next_sample, next_sample) = mpsc::channel(self.buffer_size);

        if let Some((last_step, task)) = self.active_fetch_task.take() {
            debug!("Killing previous fetch task from step {last_step}.");
            task.abort(); // we don't need it anymore :)
        }

        self.active_fetch_task = Some((
            step,
            tokio::spawn({
                debug!("New fetch task for step {step} has been spawned");
                let data_provider = self.data_provider.clone(); // only one of these tasks will acquire the lock at once. once one dies, the lock is released for sure.
                let batch_ids_not_yet_trained_on: Arc<Mutex<BatchIdSet>> =
                    batch_ids_not_yet_trained_on.clone();
                async move {
                    loop {
                        let batch_id = {
                            match assigned_batch_ids.pop() {
                                Some(assigned) => assigned,
                                None => {
                                    // out of assigned data!
                                    return;
                                }
                            }
                        };
                        // if it's already done, skip it
                        if !batch_ids_not_yet_trained_on
                            .lock()
                            .await
                            .contains(&batch_id)
                        {
                            continue;
                        }
                        debug!("Fetching data for batch: step: {step} id: {batch_id}");
                        let data_indicies_per_batch = data_indicies_per_batch as u64;
                        let start_data_id =
                            (u64::from(batch_id) * data_indicies_per_batch) as usize;
                        let data_ids = (start_data_id
                            ..(start_data_id + data_indicies_per_batch as usize))
                            .map(|d| BatchId::from_u64(d as u64))
                            .collect::<Vec<_>>();

                        match data_provider.lock().await.get_samples(&data_ids).await {
                            Ok(batch) => {
                                if !batch_ids_not_yet_trained_on
                                    .lock()
                                    .await
                                    .contains(&batch_id)
                                {
                                    // in the time between picking the sample and fetching it, someone else trained on it.
                                    // go pick another one.
                                    continue;
                                }
                                if tx_next_sample
                                    .send(Batch {
                                        id: batch_id,
                                        data: batch,
                                    })
                                    .await
                                    .is_err()
                                {
                                    debug!("Data loop finished");
                                    return;
                                }
                            }
                            Err(err) => {
                                error!("Data fetch error: {}", err);
                                return;
                            }
                        }
                    }
                }
                .instrument(info_span!("fetch_data"))
            }),
        ));

        TrainingDataForStep {
            step,
            next_sample,
            num_all_batch_ids,
            batch_ids_not_yet_trained_on,
        }
    }
}

pub struct TrainingDataForStep {
    pub step: u32,
    pub num_all_batch_ids: usize,
    pub next_sample: mpsc::Receiver<Batch>,
    pub batch_ids_not_yet_trained_on: Arc<Mutex<BatchIdSet>>,
}
