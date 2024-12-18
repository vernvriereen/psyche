use psyche_centralized_server::app::App as ServerApp;
use psyche_centralized_shared::ClientId;
use psyche_coordinator::{
    model::{Model, LLM},
    Coordinator, RunState,
};
use psyche_coordinator::{Client, Round};
use std::collections::HashSet;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

use crate::{test_utils::get_free_port, COOLDOWN_TIME};
use crate::{MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, RUN_ID, WARMUP_TIME};

enum TestingQueryMsg {
    QueryClients {
        respond_to: oneshot::Sender<Vec<Client<ClientId>>>,
    },
    QueryClientsLen {
        respond_to: oneshot::Sender<usize>,
    },
    QueryPendingClients {
        respond_to: oneshot::Sender<HashSet<Client<ClientId>>>,
    },
    QueryPendingClientsLen {
        respond_to: oneshot::Sender<usize>,
    },
    QueryRunState {
        respond_to: oneshot::Sender<RunState>,
    },
    QueryRounds {
        respond_to: oneshot::Sender<[Round; 4]>,
    },
    QueryRoundsHead {
        respond_to: oneshot::Sender<u32>,
    },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
    port: u16,
}

impl CoordinatorServer {
    pub async fn default(query_chan_receiver: Receiver<TestingQueryMsg>) -> Self {
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: RUN_ID.to_string(),
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
        init_min_clients: u32,
    ) -> Self {
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: RUN_ID.to_string(),
            model: Model::LLM(LLM::dummy()),
            data_indicies_per_batch: 1,
            rounds_per_epoch: 2,
            max_round_train_time: MAX_ROUND_TRAIN_TIME,
            round_witness_time: ROUND_WITNESS_TIME,
            min_clients: init_min_clients,
            batches_per_round: 4,
            witness_nodes: 1,
            witness_quorum: 1,
            total_steps: 10,
            overlapped: false,
            cooldown_time: COOLDOWN_TIME,
            warmup_time: WARMUP_TIME,
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
            Some(init_min_clients),
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
                let clients = self.inner.get_clients();
                respond_to.send(clients).unwrap();
            }
            TestingQueryMsg::QueryClientsLen { respond_to } => {
                let clients = self.inner.get_clients();
                respond_to.send(clients.len()).unwrap();
            }
            TestingQueryMsg::QueryPendingClients { respond_to } => {
                let pending_clients = self.inner.get_pending_clients();
                respond_to.send(pending_clients).unwrap();
            }
            TestingQueryMsg::QueryPendingClientsLen { respond_to } => {
                let pending_clients = self.inner.get_pending_clients();
                respond_to.send(pending_clients.len()).unwrap();
            }
            TestingQueryMsg::QueryRunState { respond_to } => {
                let run_state = self.inner.get_run_state();
                respond_to.send(run_state).unwrap();
            }
            TestingQueryMsg::QueryRounds { respond_to } => {
                let rounds = self.inner.get_rounds();
                respond_to.send(rounds).unwrap();
            }
            TestingQueryMsg::QueryRoundsHead { respond_to } => {
                let rounds = self.inner.get_rounds_head();
                respond_to.send(rounds).unwrap();
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
        let mut server = CoordinatorServer::new(query_chan_receiver, init_min_clients).await;
        let server_port = server.port;
        tokio::spawn(async move { server.run().await });
        Self {
            query_chan_sender,
            server_port,
        }
    }

    pub async fn get_clients(&self) -> Vec<Client<ClientId>> {
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

    pub async fn get_pending_clients(&self) -> HashSet<Client<ClientId>> {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::QueryPendingClients { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_pending_clients_len(&self) -> usize {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::QueryPendingClientsLen { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_run_state(&self) -> RunState {
        let (send, recv) = oneshot::channel::<RunState>();
        let msg = TestingQueryMsg::QueryRunState { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_rounds(&self) -> [Round; 4] {
        let (send, recv) = oneshot::channel::<[Round; 4]>();
        let msg = TestingQueryMsg::QueryRounds { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Actor task has been killed")
    }

    pub async fn get_rounds_head(&self) -> u32 {
        let (send, recv) = oneshot::channel::<u32>();
        let msg = TestingQueryMsg::QueryRoundsHead { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Actor task has been killed")
    }
}
