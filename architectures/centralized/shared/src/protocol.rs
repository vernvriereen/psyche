use std::fmt::Display;

use anyhow::anyhow;
use psyche_client::ClientId;
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_core::NodeIdentity;
use psyche_network::{NodeId, PeerList, PublicKey, SecretKey, SignedMessage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Join { run_id: String },
    Witness(Witness),
    HealthCheck(HealthChecks),
    Checkpoint(model::Checkpoint),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    P2PConnect(PeerList),
    Coordinator(Box<Coordinator<ClientId>>),
}
