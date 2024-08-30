use futures_util::future::select_all;
use iroh::base::ticket::BlobTicket;
use iroh::blobs::get::db::DownloadProgress;
use iroh::client::blobs::DownloadProgress as DownloadProgressStream;
use iroh::net::key::PublicKey;
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

use crate::util::convert_bytes;

#[derive(Debug)]
pub struct Download {
    from: PublicKey,
    hash: String,
    download: DownloadProgressStream,
    last_offset: u64,
    total_size: u64,
}

impl Download {
    pub fn new(from: PublicKey, blob_ticket: BlobTicket, download: DownloadProgressStream) -> Self {
        Self {
            from,
            hash: blob_ticket.hash().to_string(),
            download,
            last_offset: 0,
            total_size: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadUpdate {
    pub hash: String,
    pub from: PublicKey,
    pub downloaded_size_delta: u64,
    pub downloaded_size: u64,
    pub total_size: u64,
}

#[derive(Default)]
pub struct DownloadManager {
    downloads: Vec<Download>,
}

impl DownloadManager {
    pub fn add(
        &mut self,
        from: PublicKey,
        blob_ticket: BlobTicket,
        progress: DownloadProgressStream,
    ) {
        self.downloads
            .push(Download::new(from, blob_ticket, progress));
    }

    pub async fn poll_next(&mut self) -> Option<DownloadUpdate> {
        if self.downloads.is_empty() {
            return None;
        }

        let mut futures: Vec<_> = self
            .downloads
            .iter_mut()
            .map(|download| Box::pin(download.download.next()))
            .collect();

        let (result, index, _) = select_all(&mut futures).await;

        match result {
            Some(Ok(progress)) => {
                let download = &mut self.downloads[index];
                Self::handle_progress(download, progress)
            }
            Some(Err(e)) => {
                error!("Download error: {}", e);
                self.downloads.swap_remove(index);
                None
            }
            None => {
                self.downloads.swap_remove(index);
                None
            }
        }
    }

    fn handle_progress(
        download: &mut Download,
        progress: DownloadProgress,
    ) -> Option<DownloadUpdate> {
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
