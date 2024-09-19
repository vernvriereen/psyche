use anyhow::Result;
pub trait NodeIdentity:
    std::fmt::Debug + PartialEq + Eq + std::hash::Hash + Clone + Send + Sync + 'static
{
    type PrivateKey: Send + Sync;
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self>;
    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8>;
}
