use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use psyche_centralized_client::app::{AppBuilder, AppParams};
use psyche_centralized_server::app::DataServerInfo;
use psyche_data_provider::TokenSize;
use psyche_network::SecretKey;
use tokio_util::sync::CancellationToken;

use crate::RUN_ID;
use crate::SERVER_PORT;

pub async fn assert_with_retries<T, F, Fut>(mut function: F, y: T)
where
    T: PartialEq + std::fmt::Debug,
    Fut: Future<Output = T>,
    F: FnMut() -> Fut,
{
    let retry_attempts: u64 = 15;
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

pub fn client_app_params_default_for_testing() -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        private_key: SecretKey::generate(),
        server_addr: format!("localhost:{}", SERVER_PORT).to_string(),
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
    }
}

pub fn client_app_builder_default_for_testing() -> AppBuilder {
    AppBuilder::new(client_app_params_default_for_testing())
}

pub fn data_server_info_default_for_testing() -> DataServerInfo {
    DataServerInfo {
        dir: PathBuf::from("./"),
        token_size: TokenSize::TwoBytes,
        seq_len: 2048,
        shuffle_seed: [1; 32],
    }
}
