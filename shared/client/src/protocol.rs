use psyche_coordinator::CommitteeProof;
use psyche_network::{BlobTicket, NetworkConnection};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;
pub type Committment = [u8; 32];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastMessage {
    pub step: u64,
    pub committment: Committment,
    pub ticket: BlobTicket,
    pub proof: CommitteeProof,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Payload {
    pub step: u64
}