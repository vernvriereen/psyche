use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_core::{BatchId, MerkleRoot};
use psyche_network::{BlobTicket, NetworkConnection, TransmittableDownload};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<Broadcast, TransmittableDownload>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrainingResult {
    pub batch_id: BatchId,
    pub ticket: BlobTicket,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BroadcastType {
    TrainingResult(TrainingResult),
    Finished(MerkleRoot),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Broadcast {
    pub step: u32,
    pub proof: CommitteeProof,
    pub commitment: Commitment,
    pub nonce: u32,
    pub data: BroadcastType,
}
