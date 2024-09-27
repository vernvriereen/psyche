use psyche_coordinator::CommitteeProof;
use psyche_network::{BlobTicket, NetworkConnection};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Debug)]
pub struct IndexAndCommitteeProof {
    pub index: u64,
    pub committee_proof: CommitteeProof
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastMessage {
    pub step: u64,
    pub ticket: BlobTicket,
    pub proof: IndexAndCommitteeProof,
}

#[derive(Serialize, Deserialize)]
pub struct Payload {
    pub step: u64
}