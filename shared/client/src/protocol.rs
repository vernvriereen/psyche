use crate::SerializedDistroResult;
use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_core::BatchId;
use psyche_network::{BlobTicket, NetworkConnection};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<TrainingResult, TransmittableDistroResult>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrainingResult {
    pub step: u32,
    pub batch_id: BatchId,
    pub commitment: Commitment,
    pub ticket: BlobTicket,
    pub proof: CommitteeProof,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransmittableDistroResult {
    pub step: u32,
    pub batch_id: BatchId,
    pub distro_results: Vec<SerializedDistroResult>,
}
