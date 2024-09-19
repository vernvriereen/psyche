use anyhow::anyhow;
use psyche_coordinator::Coordinator;
use psyche_core::NodeIdentity;
use psyche_network::{NetworkConnection, NodeId, PeerList, PublicKey, SecretKey, SignedMessage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct ClientId(NodeId);

pub type NC = NetworkConnection<BroadcastMessage, Payload>;

impl NodeIdentity for ClientId {
    type PrivateKey = SecretKey;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> anyhow::Result<Self> {
        let (key, decoded_challenge) = SignedMessage::<[u8; 32]>::verify_and_decode(bytes)?;
        if decoded_challenge != challenge {
            return Err(anyhow!(
                "Mismatch in decoded challenge {:?} != {:?}",
                decoded_challenge,
                challenge
            ));
        }
        Ok(Self(key))
    }

    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8> {
        assert_eq!(private_key.public(), self.0);
        SignedMessage::sign_and_encode(private_key, &challenge)
            .expect("alloc error")
            .to_vec()
    }
}

impl From<PublicKey> for ClientId {
    fn from(value: PublicKey) -> Self {
        Self(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Join { run_id: String, data_bid: u32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    P2PConnect(PeerList),
    Coordinator(Coordinator<ClientId>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastMessage {
    pub step: usize,
    pub distro_result: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
pub struct Payload {}
