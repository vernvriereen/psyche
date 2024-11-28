use std::fmt::Display;

use crate::SerializedDistroResult;
use anyhow::anyhow;
use psyche_coordinator::{Commitment, CommitteeProof};
use psyche_core::NodeIdentity;
use psyche_network::{
    BlobTicket, NetworkConnection, NetworkEvent, NodeId, PublicKey, SecretKey, SignedMessage
};
use serde::{Deserialize, Serialize};

pub type NC = NetworkConnection<BroadcastMessage, Payload>;
pub type NE = NetworkEvent<BroadcastMessage, Payload>;

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct ClientId(NodeId);

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0.fmt_short()))?;
        Ok(())
    }
}

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

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl From<PublicKey> for ClientId {
    fn from(value: PublicKey) -> Self {
        Self(value)
    }
}

impl AsRef<[u8]> for ClientId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

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
