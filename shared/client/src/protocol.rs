use psyche_coordinator::OwnedCommitteeAndWitnessWithProof;
use psyche_network::{BlobTicket, NetworkConnection};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastMessage {
    pub step: u32,
    pub committee: OwnedCommitteeAndWitnessWithProof,
    pub ticket: BlobTicket,
}

#[derive(Serialize, Deserialize)]
pub struct Payload {
    pub step: u32
}