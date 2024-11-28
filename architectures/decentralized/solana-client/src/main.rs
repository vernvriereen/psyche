mod backend;

use anyhow::Result;
use backend::SolanaBackend;
use solana_sdk::{signature::Keypair, signer::EncodableKey};
use tokio;

#[tokio::main]
pub async fn main() -> Result<()> {
    let key_pair = Keypair::read_from_file("../../.config/solana/id.json").unwrap();
    let mut backend = SolanaBackend::new(
        "http://127.0.0.1:8899".to_string(),
        "2mQJR6fyjAJwoevxzZVLW6ReLenK1dxPzDmVTVMW5AKx".to_string(),
        key_pair,
    )
    .expect("Failed to create Solana client backend");
    backend.send_transacion_test().await
}
