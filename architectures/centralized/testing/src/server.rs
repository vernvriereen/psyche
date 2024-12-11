use psyche_centralized_server::app::{App as ServerApp, DataServerInfo};
use psyche_centralized_shared::ClientId;
use psyche_coordinator::{
    model::{Model, LLM},
    Coordinator, RunState,
};
use std::fs::File;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
};

use crate::{
    test_utils::{data_server_info_default_for_testing, repo_path},
    RUN_ID, SERVER_PORT, WARMUP_TIME,
};

enum TestingQueryMsg {
    QueryClients {
        respond_to: oneshot::Sender<usize>,
    },
    QueryRunState {
        respond_to: oneshot::Sender<RunState>,
    },
}

struct CoordinatorServer {
    inner: ServerApp,
    query_chan_receiver: Receiver<TestingQueryMsg>,
}

impl CoordinatorServer {
    pub async fn default(query_chan_receiver: Receiver<TestingQueryMsg>) -> Self {
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: RUN_ID.to_string(),
            model: Model::LLM(LLM::dummy()),
            ..Default::default()
        };

        let server = ServerApp::new(
            false,
            coordinator,
            Some(data_server_info_default_for_testing()),
            None,
            Some(SERVER_PORT),
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

    pub async fn new(
        query_chan_receiver: Receiver<TestingQueryMsg>,
        init_min_clients: Option<u32>,
    ) -> Self {
        let coordinator: Coordinator<ClientId> = Coordinator {
            run_id: RUN_ID.to_string(),
            ..Default::default()
        };

        let server = ServerApp::new(
            false,
            coordinator,
            Some(data_server_info_default_for_testing()),
            None,
            Some(SERVER_PORT),
            None,
            Some(WARMUP_TIME),
            init_min_clients,
        )
        .await
        .unwrap();

        Self {
            inner: server,
            query_chan_receiver,
        }
    }

    pub async fn new_with_model(
        query_chan_receiver: Receiver<TestingQueryMsg>,
        init_min_clients: Option<u32>,
    ) -> Self {
        let repo_path = repo_path();

        let state_path = std::path::Path::new(&repo_path).join("config/testing/state.toml");
        let state_toml_bytes = std::fs::read(state_path).unwrap();
        let state_toml_string = std::str::from_utf8(&state_toml_bytes).unwrap();
        let coordinator: Coordinator<ClientId> = toml::from_str(state_toml_string).unwrap();

        let data_path = std::path::Path::new(&repo_path).join("config/testing/data.toml");
        let data_toml_bytes = std::fs::read(data_path).unwrap();
        let data_toml_string = std::str::from_utf8(&data_toml_bytes).unwrap();

        let data_server_info: DataServerInfo = toml::from_str(data_toml_string).unwrap();

        // Assert dolma data is present:
        let dolma_path =
            repo_path + "/config/testing/dolma/dolma-v1_7-30B-tokenized-llama2-nanoset.npy";
        let _dolma_data = File::open(dolma_path).expect(
            "Failed to read dolma data. Please ensure the dolma data file is located at /config/testing/dolma/.",
        );

        let server = ServerApp::new(
            false,
            coordinator,
            Some(data_server_info),
            None,
            Some(SERVER_PORT),
            None,
            Some(WARMUP_TIME),
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
                respond_to.send(clients_len).unwrap();
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
}

impl CoordinatorServerHandle {
    pub async fn default() -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::default(query_chan_receiver).await;
        tokio::spawn(async move { server.run().await });

        Self { query_chan_sender }
    }

    pub async fn new(init_min_clients: u32) -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server = CoordinatorServer::new(query_chan_receiver, Some(init_min_clients)).await;
        tokio::spawn(async move { server.run().await });
        Self { query_chan_sender }
    }

    pub async fn new_with_model(init_min_clients: u32) -> Self {
        let (query_chan_sender, query_chan_receiver) = mpsc::channel(64);
        let mut server =
            CoordinatorServer::new_with_model(query_chan_receiver, Some(init_min_clients)).await;
        tokio::spawn(async move { server.run().await });
        Self { query_chan_sender }
    }

    pub async fn get_clients_len(&self) -> usize {
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
