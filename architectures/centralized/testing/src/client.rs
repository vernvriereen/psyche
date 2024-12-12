use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_client::app::AppBuilder as ClientAppBuilder;
use psyche_centralized_shared::ClientId;
use psyche_client::RunInitConfig;
use psyche_client::NC;
use tokio::select;
use tokio::task::JoinHandle;

use crate::test_utils::client_app_params_default_for_testing;

struct Client {
    inner: ClientApp,
}

impl Client {
    pub async fn new(server_port: u16) -> (Self, NC, RunInitConfig<ClientId>) {
        let client_app_params = client_app_params_default_for_testing(server_port);
        let (client_app, p2p, state_options) = ClientAppBuilder::new(client_app_params)
            .build()
            .await
            .unwrap();

        (Self { inner: client_app }, p2p, state_options)
    }

    pub async fn run(&mut self, p2p: NC, state_options: RunInitConfig<ClientId>) {
        let client_run = self.inner.run(p2p, state_options);
        tokio::pin!(client_run);
        loop {
            select! {
                run_res = &mut client_run => run_res.unwrap(),
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ClientHandle {
    pub client_handle: JoinHandle<()>,
}

impl ClientHandle {
    pub async fn new(server_port: u16) -> Self {
        let (mut client, p2p, state_options) = Client::new(server_port).await;
        let client_handle = tokio::spawn(async move { client.run(p2p, state_options).await });
        Self { client_handle }
    }
}
