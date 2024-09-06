use iroh::net::NodeId;
use psyche_client::payload::Payload;
use psyche_coordinator::coordinator::Coordinator;
use psyche_network::NetworkConnection;
use serde::{Deserialize, Serialize};

pub type ClientId = NodeId;
pub type NC = NetworkConnection<Message, Payload>;

#[derive(Serialize, Deserialize)]
pub enum Message {
    Coordinator(Coordinator<ClientId>),
    Join,
}
