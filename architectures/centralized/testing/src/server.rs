use psyche_centralized_server::app::{App as ServerApp, DataServerInfo};
use psyche_centralized_shared::ClientId;
use psyche_coordinator::{Coordinator, RunState};
use psyche_data_provider::TokenSize;
use std::path::PathBuf;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

pub const RUN_ID: &str = "test";

enum TestingQueryMsg {
    QueryClients { respond_to: oneshot::Sender<u32> },
    QueryRunState { respond_to: oneshot::Sender<RunState> },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
}

impl CoordinatorServer {
    pub async fn new(query_chan_receiver: Receiver<TestingQueryMsg>) -> Self {
        let mut coordinator: Coordinator<ClientId> = Coordinator::default();
        coordinator.run_id = RUN_ID.to_string();

        let server = ServerApp::new(
            false,
            coordinator,
            Some(DataServerInfo::default()),
            Some(1234),
            Some(8080),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        Self {
            inner: server,
            query_chan_receiver,
        }
    }

    pub async fn new_custom(query_chan_receiver: Receiver<TestingQueryMsg>, init_min_clients: Option<u32>) -> Self {
        let mut coordinator: Coordinator<ClientId> = Coordinator::default();
        coordinator.run_id = RUN_ID.to_string();

        let server = ServerApp::new(
            false,
            coordinator,
            Some(DataServerInfo::default()),
            Some(1234),
            Some(8080),
            None,
            Some(30),
            init_min_clients,
        )
        .await
        .unwrap();

        Self {
            inner: server,
            query_chan_receiver,
        }
    }

    pub async fn handle_message(&mut self, msg: TestingQueryMsg) {
        match msg {
            TestingQueryMsg::QueryClients { respond_to } => {
                let clients_len = self.inner.get_pending_clients_len();
                let _ = respond_to.send(clients_len as u32).unwrap();
            }
            TestingQueryMsg::QueryRunState { respond_to } => {
                let run_state = self.inner.get_runstate();
                let _ = respond_to.send(run_state).unwrap();
            },
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
}

impl CoordinatorServerHandle {
    pub async fn new() -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::new(query_chan_receiver).await;
        tokio::spawn(async move { server.run().await });
        Self { query_chan_sender }
    }

    pub async fn new_custom(init_min_clients: Option<u32>) -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::new_custom(query_chan_receiver, init_min_clients).await;
        tokio::spawn(async move { server.run().await });
        Self { query_chan_sender }
    }

    pub async fn get_clients_len(&self) -> u32 {
        let (send, recv) = oneshot::channel();
        let msg = TestingQueryMsg::QueryClients { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Actor task has been killed")
    }

    pub async fn get_run_state(&self) -> RunState {
        let (send, recv) = oneshot::channel::<RunState>();
        let msg = TestingQueryMsg::QueryRunState { respond_to: send };
        let _ = self.query_chan_sender.send(msg).await;
        recv.await.expect("Actor task has been killed")
    }
}
