use std::path::PathBuf;

use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_core::{BatchId, NodeIdentity};
use psyche_modeling::DistroResult;
use psyche_network::{BlobTicket, TransmittableDistroResult};
use tch::TchError;
use thiserror::Error;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct HubUploadInfo {
    pub hub_repo: String,
    pub hub_token: String,
}

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub hub_upload: Option<HubUploadInfo>,
    pub checkpoint_dir: PathBuf,
}

#[derive(Debug)]
pub enum PayloadState<T: NodeIdentity> {
    Downloading((T, BatchId, BlobTicket)),
    Deserializing(JoinHandle<Result<Vec<DistroResult>, DeserializeError>>),
}

#[derive(Error, Debug)]
pub enum DeserializeError {
    #[error("Deserialize thread crashed")]
    DeserializeThreadCrashed,

    #[error("Deserialize error: {0}")]
    Deserialize(#[from] TchError),
}

pub struct DistroBroadcastAndPayload {
    pub step: u32,
    pub batch_id: BatchId,
    pub commitment: Commitment,
    pub proof: CommitteeProof,
    pub distro_result: TransmittableDistroResult,
}
