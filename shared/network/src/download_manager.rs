use crate::{
    p2p_model_sharing::TransmittableModelParameter, serialized_distro::TransmittableDistroResult,
    util::convert_bytes, Networkable,
};

use anyhow::{bail, Context, Error, Result};
use bytes::Bytes;
use futures_util::future::select_all;
use iroh::PublicKey;
use iroh_blobs::{get::db::DownloadProgress, ticket::BlobTicket};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, future::Future, marker::PhantomData, pin::Pin, sync::Arc};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};
use tracing::{debug, error, info, warn};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TransmittableDownload {
    DistroResult(TransmittableDistroResult),
    ModelParameter(TransmittableModelParameter),
}

#[derive(Debug)]
struct Download {
    blob_ticket: BlobTicket,
    download: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
    last_offset: u64,
    total_size: u64,
}

struct ReadingFinishedDownload {
    blob_ticket: BlobTicket,
    download: Result<oneshot::Receiver<Bytes>>,
}

impl Debug for ReadingFinishedDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadingFinishedDownload")
            .field("blob_ticket", &self.blob_ticket)
            .field("reading", &"...")
            .finish()
    }
}

impl Download {
    fn new(
        blob_ticket: BlobTicket,
        download: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
    ) -> Self {
        Self {
            blob_ticket,
            download,
            last_offset: 0,
            total_size: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadUpdate {
    pub blob_ticket: BlobTicket,
    pub downloaded_size_delta: u64,
    pub downloaded_size: u64,
    pub total_size: u64,
    pub all_done: bool,
    pub error: Option<String>,
}

pub struct DownloadComplete<D: Networkable> {
    pub hash: iroh_blobs::Hash,
    pub from: PublicKey,
    pub data: D,
}

#[derive(Debug)]
pub struct DownloadFailed {
    pub blob_ticket: BlobTicket,
    pub error: anyhow::Error,
}

impl<D: Networkable> Debug for DownloadComplete<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadComplete")
            .field("hash", &self.hash)
            .field("from", &self.from)
            .field("data", &"...")
            .finish()
    }
}

pub enum DownloadManagerEvent<D: Networkable> {
    Update(DownloadUpdate),
    Complete(DownloadComplete<D>),
    Failed(DownloadFailed),
}

impl<D: Networkable> Debug for DownloadManagerEvent<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Update(arg0) => f.debug_tuple("Update").field(arg0).finish(),
            Self::Complete(arg0) => f.debug_tuple("Complete").field(arg0).finish(),
            Self::Failed(arg0) => f.debug_tuple("Failed").field(arg0).finish(),
        }
    }
}

pub struct DownloadManager<D: Networkable> {
    downloads: Arc<Mutex<Vec<Download>>>,
    reading: Arc<Mutex<Vec<ReadingFinishedDownload>>>,
    _download_type: PhantomData<D>,
    task_handle: Option<JoinHandle<()>>,
    event_receiver: mpsc::UnboundedReceiver<DownloadManagerEvent<D>>,
    tx_new_item: mpsc::UnboundedSender<()>,
}

impl<D: Networkable> Debug for DownloadManager<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadManager")
            .field("downloads", &self.downloads)
            .field("reading", &self.reading)
            .finish()
    }
}

impl<D: Networkable + Send + 'static> DownloadManager<D> {
    pub fn new() -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let (tx_new_item, mut rx_new_item) = mpsc::unbounded_channel();

        let downloads = Arc::new(Mutex::new(Vec::new()));
        let reading = Arc::new(Mutex::new(Vec::new()));
        let mut manager = Self {
            downloads: downloads.clone(),
            reading: reading.clone(),
            _download_type: PhantomData,
            task_handle: None,
            event_receiver,
            tx_new_item,
        };

        let task_handle = tokio::spawn(async move {
            loop {
                if downloads.lock().await.is_empty()
                    && reading.lock().await.is_empty()
                    && rx_new_item.recv().await.is_none()
                {
                    // channel is closed.
                    info!("Download manager channel closed - shutting down.");
                    return;
                }

                match Self::poll_next_inner(
                    &mut *downloads.lock().await,
                    &mut *reading.lock().await,
                )
                .await
                {
                    Ok(Some(event)) => {
                        if event_sender.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Error polling next: {}", e);
                    }
                }
            }
        });

        manager.task_handle = Some(task_handle);

        Ok(manager)
    }

    pub fn add(
        &mut self,
        blob_ticket: BlobTicket,
        progress: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
    ) {
        let downloads = self.downloads.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            debug!("Adding new download: {}", blob_ticket.hash());
            downloads
                .lock()
                .await
                .push(Download::new(blob_ticket, progress));

            if let Err(e) = sender.send(()) {
                error!("{}", e);
            }
        });
    }

    pub fn read(&mut self, blob_ticket: BlobTicket, download: Result<oneshot::Receiver<Bytes>>) {
        let reading = self.reading.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            reading.lock().await.push(ReadingFinishedDownload {
                blob_ticket,
                download,
            });
            if let Err(e) = sender.send(()) {
                error!("{}", e);
            }
        });
    }

    pub async fn poll_next(&mut self) -> Option<DownloadManagerEvent<D>> {
        let event = self.event_receiver.recv().await;
        event
    }

    async fn poll_next_inner(
        downloads: &mut Vec<Download>,
        reading: &mut Vec<ReadingFinishedDownload>,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        if downloads.is_empty() && reading.is_empty() {
            return Ok(None);
        }

        #[derive(Debug)]
        enum FutureResult {
            Download(usize, Result<DownloadProgress>),
            Read(usize, Result<Bytes>),
        }

        let download_futures = downloads.iter_mut().enumerate().map(|(i, download)| {
            Box::pin(async move {
                FutureResult::Download(
                    i,
                    download
                        .download
                        .recv()
                        .await
                        .unwrap_or_else(|| bail!("download channel closed. hmm.")),
                )
            }) as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let read_futures = reading.iter_mut().enumerate().map(|(i, read)| {
            Box::pin(async move {
                FutureResult::Read(
                    i,
                    match &mut read.download {
                        Ok(download) => download.await.map_err(|e| e.into()),
                        Err(err) => Err(Error::msg(format!(
                            "Error downloading {}: {}",
                            read.blob_ticket.hash(),
                            err
                        ))),
                    },
                )
            }) as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let all_futures: Vec<Pin<Box<dyn Future<Output = FutureResult> + Send>>> =
            download_futures.chain(read_futures).collect();

        let result = select_all(all_futures).await.0;

        match result {
            FutureResult::Download(index, result) => {
                Self::handle_download_progress(downloads, result, index)
            }
            FutureResult::Read(index, result) => Self::handle_read_result(reading, result, index),
        }
    }

    fn handle_download_progress(
        downloads: &mut Vec<Download>,
        result: Result<DownloadProgress>,
        index: usize,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        let download = &mut downloads[index];
        let r = match result {
            Ok(progress) => {
                let update = match progress {
                    DownloadProgress::InitialState(_) => None,
                    DownloadProgress::FoundLocal { size, .. } => Some(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        downloaded_size_delta: 0,
                        downloaded_size: size.value(),
                        total_size: size.value(),
                        all_done: false,
                        error: None,
                    }),
                    DownloadProgress::Connected => None,
                    DownloadProgress::Found { size, .. } => {
                        download.total_size = size;
                        Some(DownloadUpdate {
                            blob_ticket: download.blob_ticket.clone(),
                            downloaded_size_delta: 0,
                            downloaded_size: 0,
                            total_size: size,
                            all_done: false,
                            error: None,
                        })
                    }
                    DownloadProgress::FoundHashSeq { .. } => None,
                    DownloadProgress::Progress { offset, .. } => {
                        let delta = offset - download.last_offset;
                        download.last_offset = offset;
                        Some(DownloadUpdate {
                            blob_ticket: download.blob_ticket.clone(),
                            downloaded_size_delta: delta,
                            downloaded_size: offset,
                            total_size: download.total_size,
                            all_done: false,
                            error: None,
                        })
                    }
                    DownloadProgress::Done { .. } => None,
                    DownloadProgress::AllDone(stats) => {
                        debug!(
                            "Downloaded (index {index}) {}, {} ",
                            download.blob_ticket.hash(),
                            convert_bytes(stats.bytes_read as f64)
                        );
                        Some(DownloadUpdate {
                            blob_ticket: download.blob_ticket.clone(),
                            downloaded_size_delta: 0,
                            downloaded_size: download.total_size,
                            total_size: download.total_size,
                            all_done: true,
                            error: None,
                        })
                    }
                    DownloadProgress::Abort(err) => {
                        warn!("Download aborted: {:?}", err);
                        Some(DownloadUpdate {
                            blob_ticket: download.blob_ticket.clone(),

                            downloaded_size_delta: 0,
                            downloaded_size: 0,
                            total_size: 0,
                            all_done: true,
                            error: Some(format!("{err}")),
                        })
                    }
                };
                Ok(update.map(DownloadManagerEvent::Update))
            }
            Err(e) => {
                error!("Download error: {}", e);
                downloads.swap_remove(index);
                Err(e)
            }
        };
        if let Ok(Some(DownloadManagerEvent::Update(DownloadUpdate { all_done, .. }))) = &r {
            if *all_done {
                debug!("Since download is complete, removing it: {index};");
                downloads.swap_remove(index);
            }
        }
        r
    }

    fn handle_read_result(
        reading: &mut Vec<ReadingFinishedDownload>,
        result: Result<Bytes>,
        index: usize,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        let downloader: ReadingFinishedDownload = reading.swap_remove(index);
        match result {
            Ok(bytes) => {
                let decoded = D::from_bytes(bytes.as_ref())
                    .with_context(|| "Failed to decode downloaded data")?;
                Ok(Some(DownloadManagerEvent::Complete(DownloadComplete {
                    data: decoded,
                    from: downloader.blob_ticket.node_addr().node_id,
                    hash: downloader.blob_ticket.hash(),
                })))
            }
            Err(e) => Ok(Some(DownloadManagerEvent::Failed(DownloadFailed {
                blob_ticket: downloader.blob_ticket,
                error: e,
            }))),
        }
    }
}
