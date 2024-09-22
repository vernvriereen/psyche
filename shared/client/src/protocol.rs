use psyche_network::NetworkConnection;
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastMessage {
    pub step: usize,
    pub distro_result: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
pub struct Payload {}