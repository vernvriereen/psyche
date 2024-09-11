use std::array;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::try_join_all;
use psyche_coordinator::{
    coordinator::{Client, Coordinator, Round, RunState},
    traits::{NodeIdentity, WatcherBackend},
};
use psyche_core::serde::Networkable;
use psyche_data_provider::{DataProvider, DataProviderTcpClient, DataProviderTcpServer};
use psyche_tui::init_logging;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tracing::info;

// Simulated backend for demonstration
struct DummyBackend<T: NodeIdentity>(Vec<T>);

#[async_trait]
impl<T: NodeIdentity> WatcherBackend<T> for DummyBackend<T> {
    async fn wait_for_new_state(&self) -> Coordinator<T> {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        info!("new step!");
        Coordinator {
            tick: 0,
            step: 0,
            run_state: RunState::WaitingForMembers,
            run_state_start_unix_timestamp: 0,

            warmup_time: 0,

            max_rounds: 0,
            max_round_time: 0,
            rounds: array::from_fn(|_| Round {
                height: 0,
                clients_len: 0,
                data_index: 0,
                random_seed: 0,
            }),
            rounds_head: 0,

            min_clients: 0,
            clients: self.0.iter().map(|i| Client { id: i.clone() }).collect(),
            dropped_clients: Vec::new(),

            last_tick_unix_timestamp: 0,
            last_step_unix_timestamp: 0,

            data_indicies_per_round: 0,
            verification_percent: 0,

            epoch: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
struct DummyNodeIdentity(u64);
impl NodeIdentity for DummyNodeIdentity {
    type PrivateKey = ();
    fn from_signed_bytes(bytes: &[u8], challenge: [u8; 32]) -> Result<Self> {
        let (serialized_challenge, bytes) = bytes.split_at(32);
        if challenge != serialized_challenge {
            return Err(anyhow!("challenge doesn't match serialized challenge: {challenge:?} != {serialized_challenge:?}"));
        }
        Self::from_bytes(bytes)
    }

    fn to_signed_bytes(&self, private_key: &(), challenge: [u8; 32]) -> Vec<u8> {
        let mut b = challenge.to_vec();
        b.extend(self.to_bytes());
        b
    }
}

struct DummyDataProvider;
impl DataProvider for DummyDataProvider {
    async fn get_raw_sample(&self, _data_id: usize) -> Result<Vec<u8>> {
        let mut data: [u8; 1024] = [0; 1024];
        rand::thread_rng().fill_bytes(&mut data);
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
        let mut server = DataProviderTcpServer::new(local_data_provider, backend);
        server.run(5740).await
    });

    let clients = try_join_all(
        clients
            .into_iter()
            .map(|i| DataProviderTcpClient::connect("localhost:5740", i, ())),
    )
    .await?;
    info!("clients initialized successfully");
    loop {
        for (i, c) in clients.iter().enumerate() {
            c.get_raw_sample(0).await?;
            info!("client {} got data! ", i);
        }
    }
}
