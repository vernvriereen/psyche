use crate::app::{AppBuilder, AppParams, Tabs, TAB_NAMES};

use anyhow::Result;
use clap::{ArgAction, Parser};
use psyche_network::SecretKey;
use psyche_tui::{maybe_start_render_loop, LogOutput};
use std::path::PathBuf;
use tokio::runtime::Builder;
use tracing::{info, Level};

mod app;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,

    #[clap(short, long)]
    bind_p2p_port: Option<u16>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    tui: bool,

    #[clap(long)]
    run_id: String,

    #[clap(long)]
    server_addr: String,

    #[clap(long, default_value_t = 1)]
    data_parallelism: usize,

    #[clap(long, default_value_t = 1)]
    tensor_parallelism: usize,

    #[clap(long)]
    micro_batch_size: Option<usize>,

    /// If provided, every shared gradient this client sees will be written to this directory.
    #[clap(long)]
    write_gradients_dir: Option<PathBuf>,

    #[clap(long)]
    eval_tasks: Option<String>,
}

async fn async_main() -> Result<()> {
    let args = Args::parse();

    #[cfg(target_os = "windows")]
    {
        // this is a gigantic hack to cover that called sdpa prints out
        // "Torch was not compiled with flash attention." via TORCH_WARN
        // on Windows, which screws with the TUI.
        // it's done once (really TORCH_WARN_ONCE), so elicit that behavior
        // before starting anything else
        use tch::Tensor;
        let device = tch::Device::Cuda(0);
        let _ = Tensor::scaled_dot_product_attention::<Tensor>(
            &Tensor::from_slice2(&[[0.]]).to(device),
            &Tensor::from_slice2(&[[0.]]).to(device),
            &Tensor::from_slice2(&[[0.]]).to(device),
            None,
            0.0,
            false,
            None,
        );
    }

    psyche_tui::init_logging(
        if args.tui {
            LogOutput::TUI
        } else {
            LogOutput::Console
        },
        Level::INFO,
    );

    info!("joining gossip room");

    let secret_key: SecretKey = args
        .secret_key
        .map(|k| k.parse().unwrap())
        .unwrap_or_else(SecretKey::generate);

    let tui = args.tui;

    let (cancel, tx_tui_state) =
        maybe_start_render_loop(tui.then(|| Tabs::new(Default::default(), &TAB_NAMES)))?;

    AppBuilder::new(AppParams {
        cancel,
        secret_key,
        server_addr: args.server_addr,
        tx_tui_state,
        run_id: args.run_id,
        p2p_port: args.bind_p2p_port,
        data_parallelism: args.data_parallelism,
        tensor_parallelism: args.tensor_parallelism,
        micro_batch_size: args.micro_batch_size,
        write_gradients_dir: args.write_gradients_dir,
        eval_tasks: args.eval_tasks,
    })
    .run()
    .await
}

fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(8192)
        .build()
        .unwrap();
    runtime.block_on(async_main())
}
