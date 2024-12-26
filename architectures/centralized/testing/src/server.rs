use bytemuck::Zeroable;
use psyche_centralized_server::app::App as ServerApp;
use psyche_centralized_shared::ClientId;
use psyche_coordinator::{
    model::{Model, LLM},
    CoodinatorConfig, Coordinator, CoordinatorEpochState, RunState,
};
use psyche_coordinator::{Client, Round};
use rand::distributions::{Alphanumeric, DistString};
use std::collections::HashSet;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

use crate::{test_utils::get_free_port, COOLDOWN_TIME};
use crate::{MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME, WARMUP_TIME};

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
    QueryRounds {
        respond_to: oneshot::Sender<[Round; 4]>,
    },
    QueryRoundsHead {
        respond_to: oneshot::Sender<u32>,
    },
    QueryEpoch {
        respond_to: oneshot::Sender<u32>,
    },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
    port: u16,
    run_id: String,
}

fn to_fixed_size_array(s: &str) -> [u8; 64] {
    let mut array = [0u8; 64];
    let bytes = s.as_bytes();
    let len = bytes.len().min(64);
    array[..len].copy_from_slice(&bytes[..len]);
    array
}

impl CoordinatorServer {
    pub async fn new(
        query_chan_receiver: Receiver<TestingQueryMsg>,
        init_min_clients: u32,
        batches_per_round: u32,
    ) -> Self {
        let coordinator_config = CoodinatorConfig {
            warmup_time: WARMUP_TIME,
            cooldown_time: COOLDOWN_TIME,
            rounds_per_epoch: 2,
            max_round_train_time: MAX_ROUND_TRAIN_TIME,
            round_witness_time: ROUND_WITNESS_TIME,
            min_clients: init_min_clients,
            batches_per_round,
            data_indicies_per_batch: 1,
            verification_percent: 0,
            witness_nodes: 1,
            witness_quorum: 1,
            total_steps: 10,
            overlapped: false,
            ..CoodinatorConfig::<ClientId>::zeroed()
        };

        let epoch_state = CoordinatorEpochState {
            first_round: true,
            ..CoordinatorEpochState::<ClientId>::zeroed()
        };

        let run_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: to_fixed_size_array(&run_id),
            model: Model::LLM(LLM::dummy()),
            config: coordinator_config,
            epoch_state,
            ..Coordinator::<ClientId>::zeroed()
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
            run_id,
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
            TestingQueryMsg::QueryRounds { respond_to } => {
                let rounds = self.inner.get_rounds();
                respond_to.send(rounds).unwrap();
            }
            TestingQueryMsg::QueryRoundsHead { respond_to } => {
                let rounds = self.inner.get_rounds_head();
                respond_to.send(rounds).unwrap();
            }
            TestingQueryMsg::QueryEpoch { respond_to } => {
                let current_epoch = self.inner.get_current_epoch();
                respond_to.send(current_epoch).unwrap();
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
    pub run_id: String,
}

impl CoordinatorServerHandle {
    pub async fn new(init_min_clients: u32, batches_per_round: u32) -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server =
            CoordinatorServer::new(query_chan_receiver, init_min_clients, batches_per_round).await;
        let server_port = server.port;
        let run_id = server.run_id.clone();
        tokio::spawn(async move { server.run().await });
        Self {
            query_chan_sender,
            server_port,
            run_id,
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

    pub async fn get_rounds(&self) -> [Round; 4] {
        let (send, recv) = oneshot::channel::<[Round; 4]>();
        let msg = TestingQueryMsg::QueryRounds { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_rounds_head(&self) -> u32 {
        let (send, recv) = oneshot::channel::<u32>();
        let msg = TestingQueryMsg::QueryRoundsHead { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }

    pub async fn get_current_epoch(&self) -> u32 {
        let (send, recv) = oneshot::channel::<u32>();
        let msg = TestingQueryMsg::QueryEpoch { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Coordinator actor task has been killed")
    }
}
