use psyche_coordinator::{
    assign_data_for_state, get_batch_ids_for_state, CommitteeSelection, Coordinator,
};
use psyche_core::NodeIdentity;
use psyche_data_provider::{DataProviderTcpClient, TokenizedDataProvider};
use rand::Rng;
use std::{collections::HashSet, sync::Arc};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tracing::{debug, error};

pub type Batch = Vec<Vec<i32>>;
pub type BatchId = u64;
pub type BatchStep = u32;
pub type BatchIdSet = HashSet<BatchId>;

pub struct DataFetcher<T: NodeIdentity> {
    data_provider: Arc<Mutex<DataProviderTcpClient<T>>>,
    active_fetch_task: Option<(BatchStep, JoinHandle<()>)>,
    buffer_size: usize,
}

impl<T: NodeIdentity> DataFetcher<T> {
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
        committee_selection: &CommitteeSelection,
        identity: &T,
    ) -> (usize, TrainingDataForStep) {
        let step = state.step;
        let data_indicies_per_batch = state.data_indicies_per_batch;

        // everyone tries to not overlap (just a hopeful guess though, not part of consensus, everyone is free to train on whatever)
        let mut assigned_batch_ids: Vec<u64> = assign_data_for_state(state, committee_selection)
            .iter()
            .filter_map(|(key, value)| match value == identity {
                true => {
                    let batch_interval = (key.start / state.data_indicies_per_batch as u64)
                        ..=(key.end / state.data_indicies_per_batch as u64);
                    Some(batch_interval)
                }
                false => None,
            })
            .flatten()
            .collect();

        // TODO: replace `get_batch_ids_for_state` with a version that's aware of training/verify/tiebreak (or use assigned_batch_ids).
        let all_batch_ids = get_batch_ids_for_state(state);
        let num_all_batch_ids = all_batch_ids.len();
        debug!("got new batch IDs for step {step} - there are {num_all_batch_ids}");
        let batch_ids_not_yet_trained_on: std::sync::Arc<Mutex<BatchIdSet>> =
            Arc::new(Mutex::new(all_batch_ids.into_iter().collect()));

        let (tx_next_sample, next_sample) = mpsc::channel(self.buffer_size);

        if let Some((last_step, task)) = self.active_fetch_task.take() {
            debug!("killing previous fetch task from step {last_step}.");
            task.abort(); // we don't need it anymore :)
        }

        self.active_fetch_task = Some((
            step,
            tokio::spawn({
                debug!("new fetch task for step {step} has been spawned");
                let data_provider = self.data_provider.clone(); // only one of these tasks will acquire the lock at once. once one dies, the lock is released for sure.
                let batch_ids_not_yet_trained_on = batch_ids_not_yet_trained_on.clone();
                async move {
                    loop {
                        let batch_id = {
                            match assigned_batch_ids.pop() {
                                Some(assigned) => assigned,
                                None => {
                                    let remaining_batch_ids =
                                        batch_ids_not_yet_trained_on.lock().await;
                                    match remaining_batch_ids.len() {
                                        0 => {
                                            break;
                                        }
                                        len => remaining_batch_ids
                                            .iter()
                                            .nth(rand::thread_rng().gen_range(0..len))
                                            .copied()
                                            .unwrap(),
                                    }
                                }
                            }
                        };
                        debug!("fetching data for batch: step: {step} id: {batch_id}");
                        let data_indicies_per_batch = data_indicies_per_batch as u64;
                        let start_data_id = (batch_id * data_indicies_per_batch) as usize;
                        let data_ids = (start_data_id
                            ..(start_data_id + data_indicies_per_batch as usize))
                            .collect::<Vec<_>>();

                        match data_provider.lock().await.get_samples(data_ids).await {
                            Ok(batch) => {
                                debug!("Sending step {step} id {batch_id}");
                                if tx_next_sample.send((batch_id, batch)).await.is_err() {
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
            }),
        ));

        (
            num_all_batch_ids,
            TrainingDataForStep {
                step,
                next_sample,
                batch_ids_not_yet_trained_on,
            },
        )
    }
}

pub struct TrainingDataForStep {
    pub step: u32,
    pub next_sample: mpsc::Receiver<(BatchId, Batch)>,
    pub batch_ids_not_yet_trained_on: Arc<Mutex<BatchIdSet>>,
}
