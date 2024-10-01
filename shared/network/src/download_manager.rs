use std::{borrow::BorrowMut, fmt::Debug, future::Future, marker::PhantomData, pin::Pin};

use crate::util::convert_bytes;
use anyhow::Result;
use bytes::Bytes;
use futures_util::future::select_all;
use iroh::base::ticket::BlobTicket;
use iroh::blobs::get::db::DownloadProgress;
use iroh::net::key::PublicKey;
use psyche_core::Networkable;
use tokio::{sync::mpsc, sync::oneshot};
use tracing::{error, info, warn};

#[derive(Debug)]
pub struct Download {
    from: PublicKey,
    hash: iroh::blobs::Hash,
    download: mpsc::Receiver<Result<DownloadProgress>>,
    last_offset: u64,
    total_size: u64,
}

pub struct ReadingFinishedDownload {
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
    pub fn new(
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
    pub fn new(
        from: PublicKey,
        hash: iroh::blobs::Hash,
        download: oneshot::Receiver<Bytes>,
    ) -> Self {
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

pub enum DownloadManagerEvent<D: Networkable> {
    Update(DownloadUpdate),
    Complete(DownloadComplete<D>),
}

pub struct DownloadManager<D: Networkable> {
    downloads: Vec<Download>,
    reading: Vec<ReadingFinishedDownload>,
    _download_type: PhantomData<D>,
}

impl<D: Networkable> Debug for DownloadManager<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadManager")
            .field("downloads", &self.downloads)
            .field("reading", &self.reading)
            .finish()
    }
}

impl<D: Networkable> Default for DownloadManager<D> {
    fn default() -> Self {
        Self {
            downloads: Default::default(),
            reading: Default::default(),
            _download_type: Default::default(),
        }
    }
}

// TODO if it takes too long to get data from one peer, we should send a gossipsub message asking for anyone that has this info, and pick a random new person to download from.
impl<D: Networkable> DownloadManager<D> {
    pub fn add(
        &mut self,
        from: PublicKey,
        blob_ticket: BlobTicket,
        progress: mpsc::Receiver<Result<DownloadProgress>>,
    ) {
        self.downloads
            .push(Download::new(from, blob_ticket, progress));
    }

    pub fn read(
        &mut self,
        from: PublicKey,
        hash: iroh::blobs::Hash,
        download: oneshot::Receiver<Bytes>,
    ) {
        self.reading
            .push(ReadingFinishedDownload::new(from, hash, download));
    }

    // TODO error handling for failed downloads - bad decode, etc.
    pub async fn poll_next(&mut self) -> Result<Option<DownloadManagerEvent<D>>> {
        if self.is_empty() {
            return Ok(None);
        }

        enum FutureResult {
            Download(usize, Option<Result<DownloadProgress>>),
            Read(usize, Result<Bytes>),
        }

        let download_futures = self.downloads.iter_mut().enumerate().map(|(i, download)| {
            Box::pin(async move { FutureResult::Download(i, download.download.recv().await) })
                as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let read_futures = self.reading.iter_mut().enumerate().map(|(i, read)| {
            Box::pin(async move {
                FutureResult::Read(i, read.download.borrow_mut().await.map_err(|e| e.into()))
            }) as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let all_futures: Vec<Pin<Box<dyn Future<Output = FutureResult> + Send>>> =
            download_futures.chain(read_futures).collect();

        let result = select_all(all_futures).await.0;
        match result {
            FutureResult::Download(index, result) => self.handle_download_result(result, index),
            FutureResult::Read(index, result) => self.handle_read_result(result, index),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.downloads.is_empty() && self.reading.is_empty()
    }

    fn handle_download_result(
        &mut self,
        result: Option<Result<DownloadProgress>>,
        index: usize,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        match result {
            Some(Ok(progress)) => Ok(self
                .handle_progress(index, progress)
                .map(DownloadManagerEvent::Update)),
            Some(Err(e)) => {
                error!("Download error: {}", e);
                self.downloads.swap_remove(index);
                Err(e.into())
            }
            None => {
                self.downloads.swap_remove(index);
                Ok(None)
            }
        }
    }

    fn handle_read_result(
        &mut self,
        result: Result<Bytes>,
        index: usize,
    ) -> Result<Option<DownloadManagerEvent<D>>> {
        let downloader = self.reading.swap_remove(index);
        let decoded = D::from_bytes(result?.as_ref())?;
        Ok(Some(DownloadManagerEvent::Complete(DownloadComplete {
            data: decoded,
            from: downloader.from,
            hash: downloader.hash,
        })))
    }

    fn handle_progress(
        &mut self,
        index: usize,
        progress: DownloadProgress,
    ) -> Option<DownloadUpdate> {
        let download = &mut self.downloads[index];
        match progress {
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
                let update = Some(DownloadUpdate {
                    hash: download.hash.clone(),
                    from: download.from,
                    downloaded_size_delta: offset - download.last_offset,
                    downloaded_size: offset,
                    total_size: download.total_size,
                });
                download.last_offset = offset;
                update
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
        }
    }
}
