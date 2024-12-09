use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_core::NodeIdentity;
use psyche_network::{
    FromSignedBytesError, NetworkableNodeIdentity, NodeId, PeerList, PublicKey, SecretKey, SignedMessage};
use std::fmt::Display;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Join { run_id: String },
    Witness(Box<Witness>),
    HealthCheck(HealthChecks),
    Checkpoint(model::Checkpoint),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    P2PConnect(PeerList),
    Coordinator(Box<Coordinator<ClientId>>),
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug, Copy)]
pub struct ClientId(NodeId);

impl Default for ClientId {
    fn default() -> Self {
        let node_id = NodeId::from_bytes(&[0; 32]).unwrap();
        Self(node_id)
    }
}

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0.fmt_short()))?;
        Ok(())
    }
}

impl NodeIdentity for ClientId {}

impl NetworkableNodeIdentity for ClientId {
    type PrivateKey = SecretKey;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self, FromSignedBytesError> {
        let (key, decoded_challenge) = SignedMessage::<[u8; 32]>::verify_and_decode(bytes)
            .map_err(|_| FromSignedBytesError::Deserialize)?;
        if decoded_challenge != challenge {
            return Err(FromSignedBytesError::MismatchedChallenge(
                challenge,
                decoded_challenge.into(),
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

impl AnchorSerialize for ClientId {
    fn serialize<W: std::io::Write>(&self, _: &mut W) -> std::io::Result<()> {
        unimplemented!()
    }
}

impl AnchorDeserialize for ClientId {
    fn deserialize_reader<R: std::io::Read>(_: &mut R) -> std::io::Result<Self> {
        unimplemented!()
    }
}

impl anchor_lang::Space for ClientId {
    const INIT_SPACE: usize = 0;
}
