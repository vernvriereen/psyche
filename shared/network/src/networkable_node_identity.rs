use psyche_core::NodeIdentity;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum FromSignedBytesError {
    #[error("bytes are not a valid NodeIdentity.")]
    Deserialize,

    #[error("challenge doesn't match expected challenge: {0:?} != {1:?}")]
    MismatchedChallenge([u8; 32], Vec<u8>),
}

pub trait NetworkableNodeIdentity: NodeIdentity {
    type PrivateKey: Send + Sync + Clone;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self, FromSignedBytesError>;
    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8>;

    fn get_p2p_public_key(&self) -> &[u8; 32];
}
