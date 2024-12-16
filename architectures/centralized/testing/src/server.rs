use psyche_centralized_server::app::App as ServerApp;
use psyche_centralized_shared::ClientId;
use psyche_coordinator::Client;
use psyche_coordinator::{
    model::{Model, LLM},
    Coordinator, RunState,
};
use std::collections::HashSet;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

use crate::test_utils::get_free_port;
use crate::{RUN_ID, WARMUP_TIME};

enum TestingQueryMsg {
    QueryClients {
        respond_to: oneshot::Sender<HashSet<Client<ClientId>>>,
    },
    QueryClientsLen {
        respond_to: oneshot::Sender<usize>,
    },
    QueryRunState {
        respond_to: oneshot::Sender<RunState>,
    },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
    port: u16,
}

fn to_fixed_size_array(s: &str) -> [u8; 64] {
    let mut array = [0u8; 64];
    let bytes = s.as_bytes();
    let len = bytes.len().min(64);
    array[..len].copy_from_slice(&bytes[..len]);
    array
}

impl CoordinatorServer {
    pub async fn default(query_chan_receiver: Receiver<TestingQueryMsg>) -> Self {
        dbg!(RUN_ID.to_string());
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: to_fixed_size_array(RUN_ID),
            model: Model::LLM(LLM::dummy()),
            data_indicies_per_batch: 1,
            ..Default::default()
        };

        let server_port = get_free_port();
        let server = ServerApp::new(
            false,
            coordinator,
            None,
            None,
            Some(server_port),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        Self {
            inner: server,
            query_chan_receiver,
            port: server_port,
        }
    }

    pub async fn new(
        query_chan_receiver: Receiver<TestingQueryMsg>,
        init_min_clients: Option<u32>,
    ) -> Self {
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: to_fixed_size_array(RUN_ID),
            model: Model::LLM(LLM::dummy()),
            data_indicies_per_batch: 1,
            rounds_per_epoch: 20,
            max_round_train_time: 3,
            round_witness_time: 2,
            min_clients: 2,
            batches_per_round: 4,
            witness_nodes: 1,
            witness_quorum: 1,
            total_steps: 10,
            overlapped: false,
            cooldown_time: 5,
            ..Default::default()
        };

        let server_port = get_free_port();
        let server = ServerApp::new(
            false,
            coordinator,
            None,
            None,
            Some(server_port),
            None,
            Some(WARMUP_TIME),
            init_min_clients,
        )
        .await
        .unwrap();

        Self {
            inner: server,
            query_chan_receiver,
            port: server_port,
        }
    }

    pub async fn handle_message(&mut self, msg: TestingQueryMsg) {
        match msg {
            TestingQueryMsg::QueryClients { respond_to } => {
                let clients = self.inner.get_pending_clients();
                respond_to.send(clients).unwrap();
            }
            TestingQueryMsg::QueryClientsLen { respond_to } => {
                let clients = self.inner.get_pending_clients();
                respond_to.send(clients.len()).unwrap();
            }
            TestingQueryMsg::QueryRunState { respond_to } => {
                let run_state = self.inner.get_run_state();
                respond_to.send(run_state).unwrap();
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            select! {
                res = self.inner.run() => res.unwrap(),
                Some(client_msg) = self.query_chan_receiver.recv() => self.handle_message(client_msg).await
            }
        }
    }
}

pub struct CoordinatorServerHandle {
    query_chan_sender: mpsc::Sender<TestingQueryMsg>,
    pub server_port: u16,
}

impl CoordinatorServerHandle {
    pub async fn default() -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::default(query_chan_receiver).await;
        let server_port = server.port;
        tokio::spawn(async move { server.run().await });

        Self {
            query_chan_sender,
            server_port,
        }
    }

    pub async fn new(init_min_clients: u32) -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::new(query_chan_receiver, Some(init_min_clients)).await;
        let server_port = server.port;
        tokio::spawn(async move { server.run().await });
        Self {
            query_chan_sender,
            server_port,
        }
    }

    pub async fn get_clients(&self) -> HashSet<Client<ClientId>> {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::QueryClients { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_clients_len(&self) -> usize {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::QueryClientsLen { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_run_state(&self) -> RunState {
        let (send, recv) = oneshot::channel::<RunState>();
        let msg = TestingQueryMsg::QueryRunState { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }
}
