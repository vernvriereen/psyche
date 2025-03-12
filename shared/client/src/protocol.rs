use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_core::BatchId;
use psyche_network::{BlobTicket, NetworkConnection, TransmittableDownload};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<TrainingResult, TransmittableDownload>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrainingResult {
    pub step: u32,
    pub batch_id: BatchId,
    pub commitment: Commitment,
    pub ticket: BlobTicket,
    pub proof: CommitteeProof,
    pub nonce: u32,
}
