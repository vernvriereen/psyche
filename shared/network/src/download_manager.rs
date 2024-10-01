use std::{fmt::Debug, future::Future, marker::PhantomData, pin::Pin, sync::Arc};

use crate::util::convert_bytes;
use anyhow::{bail, Context, Result};
use bytes::Bytes;
use futures_util::future::select_all;
use iroh::base::ticket::BlobTicket;
use iroh::blobs::get::db::DownloadProgress;
use iroh::net::key::PublicKey;
use psyche_core::Networkable;
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};
use tracing::{error, info, warn};

#[derive(Debug)]
struct Download {
    from: PublicKey,
    hash: iroh::blobs::Hash,
    download: mpsc::Receiver<Result<DownloadProgress>>,
    last_offset: u64,
    total_size: u64,
}

struct ReadingFinishedDownload {
    from: PublicKey,
    hash: iroh::blobs::Hash,
    download: oneshot::Receiver<Bytes>,
}

impl Debug for ReadingFinishedDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadingFinishedDownload")
            .field("from", &self.from)
            .field("hash", &self.hash)
            .field("reading", &"...")
            .finish()
    }
}

impl Download {
    fn new(
        from: PublicKey,
        blob_ticket: BlobTicket,
        download: mpsc::Receiver<Result<DownloadProgress>>,
    ) -> Self {
        Self {
            from,
            hash: blob_ticket.hash(),
            download,
            last_offset: 0,
            total_size: 0,
        }
    }
}

impl ReadingFinishedDownload {
    fn new(from: PublicKey, hash: iroh::blobs::Hash, download: oneshot::Receiver<Bytes>) -> Self {
        Self {
            download,
            from,
            hash,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadUpdate {
    pub hash: iroh::blobs::Hash,
    pub from: PublicKey,
    pub downloaded_size_delta: u64,
    pub downloaded_size: u64,
    pub total_size: u64,
}

pub struct DownloadComplete<D: Networkable> {
    pub hash: iroh::blobs::Hash,
    pub from: PublicKey,
    pub data: D,
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
}

impl<D: Networkable> Debug for DownloadManagerEvent<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Update(arg0) => f.debug_tuple("Update").field(arg0).finish(),
            Self::Complete(arg0) => f.debug_tuple("Complete").field(arg0).finish(),
        }
    }
}

pub struct DownloadManager<D: Networkable> {
    downloads: Arc<Mutex<Vec<Download>>>,
    reading: Arc<Mutex<Vec<ReadingFinishedDownload>>>,
    _download_type: PhantomData<D>,
    task_handle: Option<JoinHandle<()>>,
    event_receiver: mpsc::Receiver<DownloadManagerEvent<D>>,
    tx_new_item: mpsc::Sender<()>,
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
        let (event_sender, event_receiver) = mpsc::channel(100);
        let (tx_new_item, mut rx_new_item) = mpsc::channel(100);

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
                if downloads.lock().await.is_empty() && reading.lock().await.is_empty() {
                    warn!("Download manager waqiting for new item..");
                    if rx_new_item.recv().await.is_none() {
                        // channel is closed.
                        warn!("Download manager channel closed!");
                        break;
                    }
                }

                match Self::poll_next_inner(
                    &mut *downloads.lock().await,
                    &mut *reading.lock().await,
                )
                .await
                {
                    Ok(Some(event)) => {
                        if event_sender.send(event).await.is_err() {
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
        from: PublicKey,
        blob_ticket: BlobTicket,
        progress: mpsc::Receiver<Result<DownloadProgress>>,
    ) {
        let downloads = self.downloads.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            downloads
                .lock()
                .await
                .push(Download::new(from, blob_ticket, progress));
            if let Err(e) = sender.send(()).await {
                warn!("{}", e);
            }
        });
    }

    pub fn read(
        &mut self,
        from: PublicKey,
        hash: iroh::blobs::Hash,
        download: oneshot::Receiver<Bytes>,
    ) {
        let reading = self.reading.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            reading
                .lock()
                .await
                .push(ReadingFinishedDownload::new(from, hash, download));
            if let Err(e) = sender.send(()).await {
                warn!("{}", e);
            }
        });
    }

    pub async fn poll_next(&mut self) -> Option<DownloadManagerEvent<D>> {
        self.event_receiver.recv().await
    }

    async fn poll_next_inner(
        downloads: &mut Vec<Download>,
        reading: &mut Vec<ReadingFinishedDownload>,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        if downloads.is_empty() && reading.is_empty() {
            return Ok(None);
        }

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
                FutureResult::Read(i, (&mut read.download).await.map_err(|e| e.into()))
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
        match result {
            Ok(progress) => {
                let update = match progress {
                    DownloadProgress::InitialState(_) => None,
                    DownloadProgress::FoundLocal { size, .. } => Some(DownloadUpdate {
                        hash: download.hash.clone(),
                        from: download.from,
                        downloaded_size_delta: 0,
                        downloaded_size: size.value(),
                        total_size: size.value(),
                    }),
                    DownloadProgress::Connected => None,
                    DownloadProgress::Found { size, .. } => {
                        download.total_size = size;
                        Some(DownloadUpdate {
                            hash: download.hash.clone(),
                            from: download.from,
                            downloaded_size_delta: 0,
                            downloaded_size: 0,
                            total_size: size,
                        })
                    }
                    DownloadProgress::FoundHashSeq { .. } => None,
                    DownloadProgress::Progress { offset, .. } => {
                        let delta = offset - download.last_offset;
                        download.last_offset = offset;
                        Some(DownloadUpdate {
                            hash: download.hash.clone(),
                            from: download.from,
                            downloaded_size_delta: delta,
                            downloaded_size: offset,
                            total_size: download.total_size,
                        })
                    }
                    DownloadProgress::Done { .. } => None,
                    DownloadProgress::AllDone(stats) => {
                        info!("Downloaded {} ", convert_bytes(stats.bytes_read as f64));
                        Some(DownloadUpdate {
                            hash: download.hash.clone(),
                            from: download.from,
                            downloaded_size_delta: 0,
                            downloaded_size: download.total_size,
                            total_size: download.total_size,
                        })
                    }
                    DownloadProgress::Abort(err) => {
                        warn!("Download aborted: {:?}", err);
                        Some(DownloadUpdate {
                            hash: download.hash.clone(),
                            from: download.from,
                            downloaded_size_delta: 0,
                            downloaded_size: 0,
                            total_size: 0,
                        })
                    }
                };
                Ok(update.map(DownloadManagerEvent::Update))
            }
            Err(e) => {
                error!("Download error: {}", e);
                downloads.swap_remove(index);
                Err(e.into())
            }
        }
    }

    fn handle_read_result(
        reading: &mut Vec<ReadingFinishedDownload>,
        result: Result<Bytes>,
        index: usize,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        let downloader = reading.swap_remove(index);
        match result {
            Ok(bytes) => {
                let decoded = D::from_bytes(bytes.as_ref())
                    .with_context(|| "Failed to decode downloaded data")?;
                Ok(Some(DownloadManagerEvent::Complete(DownloadComplete {
                    data: decoded,
                    from: downloader.from,
                    hash: downloader.hash,
                })))
            }
            Err(e) => {
                error!("Read error: {}", e);
                Err(e.into())
            }
        }
    }
}
