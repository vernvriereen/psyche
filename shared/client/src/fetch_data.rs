use psyche_core::NodeIdentity;
use psyche_data_provider::{DataProviderTcpClient, TokenizedDataProvider};
use rand::Rng;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{error, debug};

pub type Batch = Vec<Vec<i32>>;
pub type BatchId = u64;
pub type BatchIdSet = HashSet<BatchId>;

pub fn fetch_data<T: NodeIdentity>(
    mut data_provider: DataProviderTcpClient<T>,
    notify_new_batch: Arc<Notify>,
    data_indicies_per_batch: u32,
    remaining_batch_ids: std::sync::Arc<Mutex<BatchIdSet>>,
    buffer_size: usize,
) -> mpsc::Receiver<(BatchId, Batch)> {
    let (tx, rx) = mpsc::channel(buffer_size);
    tokio::spawn(async move {
        loop {
            notify_new_batch.notified().await;
            loop {
                let batch_id = {
                    let remaining_batch_ids = remaining_batch_ids.lock().await;
                    match remaining_batch_ids.len() {
                        0 => {
                            break;
                        }
                        len => remaining_batch_ids
                            .iter()
                            .nth(rand::thread_rng().gen_range(0..len))
                            .map(|x| *x)
                            .unwrap(),
                    }
                };
                let data_indicies_per_batch = data_indicies_per_batch as u64;
                let start_data_id = (batch_id * data_indicies_per_batch) as usize;
                let data_ids = (start_data_id..(start_data_id + data_indicies_per_batch as usize))
                    .collect::<Vec<_>>();

                match data_provider.get_samples(data_ids).await {
                    Ok(batch) => {
                        if tx.send((batch_id, batch)).await.is_err() {
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
    });
    rx
}
