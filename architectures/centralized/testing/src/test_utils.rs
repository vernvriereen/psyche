use std::future::Future;
use std::net::TcpListener;
use std::time::Duration;

use psyche_centralized_client::app::AppParams;
use psyche_network::SecretKey;
use std::env;
use tokio_util::sync::CancellationToken;

use crate::client::ClientHandle;
use crate::RUN_ID;

pub fn repo_path() -> String {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    std::path::Path::new(&cargo_manifest_dir)
        .ancestors()
        .nth(3)
        .expect("Failed to determine repository root")
        .to_str()
        .unwrap()
        .to_string()
}

pub async fn spawn_clients(num_clients: usize, server_port: u16) -> Vec<ClientHandle> {
    let mut client_handles = Vec::new();
    for _ in 0..num_clients {
        client_handles.push(ClientHandle::default(server_port).await)
    }
    client_handles
}

pub async fn assert_with_retries<T, F, Fut>(mut function: F, y: T)
where
    T: PartialEq + std::fmt::Debug,
    Fut: Future<Output = T>,
    F: FnMut() -> Fut,
{
    let retry_attempts: u64 = 10;
    let mut result;
    for attempt in 1..=retry_attempts {
        result = function().await;
        if result == y {
            return;
        } else if attempt == retry_attempts {
            panic!("assertion failed {:?} != {:?}", result, y);
        } else {
            tokio::time::sleep(Duration::from_millis(250 * attempt)).await;
        }
    }
}

pub fn get_free_port() -> u16 {
    // Get a free port by binding to "0.0.0.0:0"
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    // Retrieve the assigned port number
    listener.local_addr().unwrap().port()
}

pub fn dummy_client_app_params_with_training_delay(
    server_port: u16,
    training_delay_secs: u64,
) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        private_key: SecretKey::generate(),
        server_addr: format!("localhost:{}", server_port).to_string(),
        tx_tui_state: None,
        run_id: RUN_ID.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: None,
        write_gradients_dir: None,
        p2p_port: None,
        eval_tasks: Vec::new(),
        eval_task_max_docs: None,
        checkpoint_upload_info: None,
        hub_read_token: None,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: Some(training_delay_secs),
    }
}

pub fn dummy_client_app_params_default(server_port: u16) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        private_key: SecretKey::generate(),
        server_addr: format!("localhost:{}", server_port).to_string(),
        tx_tui_state: None,
        run_id: RUN_ID.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: None,
        write_gradients_dir: None,
        p2p_port: None,
        eval_tasks: Vec::new(),
        eval_task_max_docs: None,
        checkpoint_upload_info: None,
        hub_read_token: None,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: None,
    }
}
