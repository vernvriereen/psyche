use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::try_join_all;
use parquet::data_type::AsBytes;
use psyche_coordinator::{Coordinator, HealthChecks, Witness};
use psyche_core::{Networkable, NodeIdentity};
use psyche_data_provider::{
    DataProviderTcpClient, DataProviderTcpServer, LengthKnownDataProvider, TokenizedDataProvider,
};
use psyche_tui::init_logging;
use psyche_watcher::Backend as WatcherBackend;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, usize};
use tracing::{info, Level};

// Simulated backend for demonstration
#[allow(dead_code)]
struct DummyBackend<T: NodeIdentity>(Vec<T>);

#[async_trait]
impl<T: NodeIdentity> WatcherBackend<T> for DummyBackend<T> {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<T>> {
        Ok(Coordinator::default())
    }

    async fn send_witness(&mut self, _witness: Witness) -> Result<()> {
        assert!(false, "Data provider does not send witnesses");
        Ok(())
    }

    async fn send_health_check(&mut self, _health_checks: HealthChecks) -> Result<()> {
        assert!(false, "Data provider does not send health check");
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
struct DummyNodeIdentity(u64);

impl Display for DummyNodeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))?;
        Ok(())
    }
}

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

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        todo!()
    }
}

impl AsRef<[u8]> for DummyNodeIdentity {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

struct DummyDataProvider;
impl TokenizedDataProvider for DummyDataProvider {
    async fn get_samples(&mut self, _data_ids: Vec<usize>) -> Result<Vec<Vec<i32>>> {
        let mut data: [i32; 1024] = [0; 1024];
        rand::thread_rng().fill(&mut data);
        Ok(vec![data.to_vec()])
    }
}

impl LengthKnownDataProvider for DummyDataProvider {
    fn len(&self) -> usize {
        usize::MAX
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging(psyche_tui::LogOutput::Console, Level::INFO);

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
            c.get_samples(vec![0]).await?;
            info!("client {} got data! ", i);
        }
    }
}
