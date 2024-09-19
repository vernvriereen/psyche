use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::try_join_all;
use psyche_coordinator::Coordinator;
use psyche_core::{Networkable, NodeIdentity};
use psyche_data_provider::{DataProviderTcpClient, DataProviderTcpServer, TokenizedDataProvider};
use psyche_tui::init_logging;
use psyche_watcher::Backend as WatcherBackend;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::info;

// Simulated backend for demonstration
#[allow(dead_code)]
struct DummyBackend<T: NodeIdentity>(Vec<T>);

#[async_trait]
impl<T: NodeIdentity> WatcherBackend<T> for DummyBackend<T> {
    async fn wait_for_new_state(&self) -> Coordinator<T> {
        Coordinator::default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
struct DummyNodeIdentity(u64);
impl NodeIdentity for DummyNodeIdentity {
    type PrivateKey = ();
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self> {
        let (serialized_challenge, bytes) = bytes.split_at(32);
        if challenge != serialized_challenge {
            bail!("challenge doesn't match serialized challenge: {challenge:?} != {serialized_challenge:?}");
        }
        Self::from_bytes(bytes)
    }

    fn to_signed_bytes(&self, _private_key: &(), challenge: [u8; 32]) -> Vec<u8> {
        let mut b = challenge.to_vec();
        b.extend(self.to_bytes());
        b
    }
}

struct DummyDataProvider;
impl TokenizedDataProvider for DummyDataProvider {
    async fn get_sample(&mut self, _data_id: usize) -> Result<Vec<i32>> {
        let mut data: [i32; 1024] = [0; 1024];
        rand::thread_rng().fill(&mut data);
        Ok(data.to_vec())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging(psyche_tui::LogOutput::Console);

    let clients: Vec<_> = (0..4).map(DummyNodeIdentity).collect();
    let backend = DummyBackend(clients.clone());

    tokio::spawn(async move {
        let local_data_provider = DummyDataProvider;
        let mut server = DataProviderTcpServer::start(local_data_provider, backend, 5740)
            .await
            .unwrap();
        loop {
            server.poll().await;
        }
    });

    let mut clients = try_join_all(
        clients
            .into_iter()
            .map(|i| DataProviderTcpClient::connect("localhost:5740", i, ())),
    )
    .await?;
    info!("clients initialized successfully");
    loop {
        for (i, c) in clients.iter_mut().enumerate() {
            c.get_sample(0).await?;
            info!("client {} got data! ", i);
        }
    }
}
