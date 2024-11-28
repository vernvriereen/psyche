use psyche_centralized_client::app::{AppBuilder, AppParams};
use psyche_client::BatchShuffleType;
use psyche_network::SecretKey;
use tokio_util::sync::CancellationToken;

use crate::RUN_ID;
use crate::SERVER_PORT;

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
        batch_shuffle_type: BatchShuffleType::Fixed([0; 32]),
        optim_stats: None,
        grad_accum_in_fp32: false,
    }
}

pub fn client_app_builder_default_for_testing() -> AppBuilder {
    AppBuilder::new(client_app_params_default_for_testing())
}
