use anchor_client::solana_sdk::{
    signature::{Keypair, Signature, SIGNATURE_BYTES},
    signer::Signer,
};
use anyhow::Result;
use psyche_core::NodeIdentity;
use psyche_network::{AuthenticatableIdentity, FromSignedBytesError, SecretKey, SignedMessage};
use std::sync::Arc;

#[derive(Clone, Debug, Copy)]
pub struct NetworkIdentity(solana_coordinator::ClientId);

impl AsRef<[u8]> for NetworkIdentity {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl std::hash::Hash for NetworkIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::cmp::PartialEq for NetworkIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::cmp::Eq for NetworkIdentity {}

impl std::fmt::Display for NetworkIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AuthenticatableIdentity for NetworkIdentity {
    type PrivateKey = (Arc<Keypair>, SecretKey);

    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self, FromSignedBytesError> {
        let (p2p_identity, decoded_challenge) = SignedMessage::<Vec<u8>>::verify_and_decode(bytes)
            .map_err(|_| FromSignedBytesError::Deserialize)?;
        if decoded_challenge.len() != SIGNATURE_BYTES + 32 {
            return Err(FromSignedBytesError::Deserialize);
        }
        let (signature, pubkey) = decoded_challenge.split_at(SIGNATURE_BYTES);
        let signature: Signature = signature.try_into().unwrap();
        if !signature.verify(pubkey, &challenge) {
            return Err(FromSignedBytesError::MismatchedChallenge(
                challenge,
                decoded_challenge,
            ));
        }
        let mut owner: [u8; 32] = [0; 32];
        owner.copy_from_slice(pubkey);
        Ok(Self(solana_coordinator::ClientId {
            owner: owner.into(),
            p2p_identity: *p2p_identity.as_bytes(),
        }))
    }

    fn to_signed_bytes(&self, private_key: &Self::PrivateKey, challenge: [u8; 32]) -> Vec<u8> {
        assert_eq!(private_key.0.pubkey(), self.0.owner);
        assert_eq!(private_key.1.public().as_bytes(), &self.0.p2p_identity);
        let challenge = private_key.0.sign_message(&challenge);
        SignedMessage::<Vec<u8>>::sign_and_encode(
            &private_key.1,
            &[challenge.as_ref(), &self.0.owner.to_bytes()].concat(),
        )
        .expect("alloc error")
        .to_vec()
    }

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        self.0.get_p2p_public_key()
    }
}

impl From<solana_coordinator::ClientId> for NetworkIdentity {
    fn from(value: solana_coordinator::ClientId) -> Self {
        Self(value)
    }
}
