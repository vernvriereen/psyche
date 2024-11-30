mod backend;

use anchor_client::{
    solana_sdk::{signature::Keypair, signer::EncodableKey},
    Cluster,
};
use anyhow::Result;
use backend::SolanaBackend;
use tokio;

#[tokio::main]
pub async fn main() -> Result<()> {
    let key_pair =
        Keypair::read_from_file(home::home_dir().unwrap().join(".config/solana/id.json")).unwrap();
    let mut backend = SolanaBackend::new(Cluster::Localnet, key_pair)
        .expect("Failed to create Solana client backend");
    backend.send_transacion_test().await
}
