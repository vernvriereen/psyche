use crate::SerializedDistroResult;

use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_network::{BlobTicket, NetworkConnection, NetworkEvent};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;
pub type NE = NetworkEvent<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrainingResult {
    pub step: u32,
    pub batch_id: u64,
    pub commitment: Commitment,
    pub ticket: BlobTicket,
    pub proof: CommitteeProof,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerAnnouncement {
    pub ticket: BlobTicket,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BroadcastMessage {
    TrainingResult(TrainingResult),
    PeerAnnouncement(PeerAnnouncement),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistroResult {
    pub step: u32,
    pub batch_id: u64,
    pub distro_results: Vec<SerializedDistroResult>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Payload {
    DistroResult(DistroResult),
    Empty { random: [u8; 32] },
}
