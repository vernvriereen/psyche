use anyhow::anyhow;
use iroh::net::{key::SecretKey, NodeId};
use psyche_centralized_shared::Payload;
use psyche_coordinator::{Coordinator, NodeIdentity};
use psyche_network::{NetworkConnection, SignedMessage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct ClientId(NodeId);

pub type NC = NetworkConnection<Message, Payload>;

impl NodeIdentity for ClientId {
    type PrivateKey = SecretKey;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> anyhow::Result<Self> {
        let (key, decoded_challenge) = SignedMessage::<[u8; 32]>::verify_and_decode(&bytes)?;
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

#[derive(Serialize, Deserialize)]
pub enum Message {
    Coordinator(Coordinator<ClientId>),
    Join,
}
