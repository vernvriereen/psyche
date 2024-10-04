use crate::SerializedDistroResult;
use psyche_coordinator::{CommitteeProof, Commitment};
use psyche_network::{BlobTicket, NetworkConnection, NetworkEvent};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;
pub type NE = NetworkEvent<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastMessage {
    pub step: u64,
    pub batch_id: u64,
    pub commitment: Commitment,
    pub ticket: BlobTicket,
    pub proof: CommitteeProof,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Payload {
    pub step: u64,
    pub distro_results: Vec<SerializedDistroResult>,
}
