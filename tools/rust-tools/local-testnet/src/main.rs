use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser};
use rand::seq::SliceRandom;
use serde::Deserialize;
use std::ffi::OsString;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of clients
    #[clap(long, value_parser = validate_num_clients)]
    num_clients: usize,

    /// Config directory path
    #[clap(long,value_parser = validate_config_path)]
    config_path: PathBuf,

    /// Write DisTrO data to disk
    #[clap(long)]
    write_distro_data: Option<PathBuf>,

    /// Server port
    #[clap(long, default_value_t = 20000)]
    server_port: u16,

    /// Enable TUI
    #[clap(
            long,
            action = ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            require_equals = false,
            env
        )]
    tui: bool,

    /// Force listed clients to use the same random data shuffle, causing them to train on duplicate data.
    #[clap(long, value_delimiter = ',', default_values_t = &[])]
    force_same_shuffle: Vec<usize>,

    /// Kill N clients randomly every <RANDOM_KILL_INTERVAL> seconds
    #[clap(long)]
    random_kill_num: Option<usize>,

    #[clap(long, default_value_t = 120)]
    /// Kill <RANDOM_KILL_NUM> clients randomly every N seconds
    random_kill_interval: u64,

    #[clap(long, default_value = "info,psyche=debug")]
    log: String,

    /// HF repo for the first client to checkpoint at
    #[clap(long)]
    first_client_checkpoint: Option<String>,

    #[clap(long)]
    hf_token: Option<String>,

    #[clap(long, default_value_t = false)]
    write_log: bool,
}

fn validate_num_clients(s: &str) -> Result<usize> {
    let n: usize = s
        .parse()
        .context("NUM_CLIENTS must be a positive integer")?;
    if n > 0 {
        Ok(n)
    } else {
        bail!("NUM_CLIENTS must be a positive integer")
    }
}

fn validate_config_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("Config path {} does not exist", s))
    }
}

#[derive(Deserialize)]
struct TomlWithRunId {
    run_id: String,
}

fn extract_run_id(state_path: &PathBuf) -> Result<String> {
    let toml: TomlWithRunId = toml::from_str(&std::fs::read_to_string(state_path)?)?;
    Ok(toml.run_id)
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(n_kill) = args.random_kill_num {
        if n_kill > args.num_clients {
            bail!(
                "You requested to kill {n_kill} clients randomly, but you only have {} clients.",
                args.num_clients
            );
        }
    }
    let state_path = args.config_path.join("state.toml");
    let data_path = args.config_path.join("data.toml");

    // Pre-build packages
    Command::new("cargo")
        .args(["build", "-p", "psyche-centralized-server"])
        .status()
        .expect("Failed to build server");

    Command::new("cargo")
        .args(["build", "-p", "psyche-centralized-client"])
        .status()
        .expect("Failed to build client");

    // Validate config
    Command::new("cargo")
        .args([
            "run",
            "-p",
            "psyche-centralized-server",
            "--",
            "--state",
            state_path.to_str().unwrap(),
            "--data-config",
            data_path.to_str().unwrap(),
            "validate-config",
        ])
        .status()
        .expect("Failed to validate config");

    let run_id = extract_run_id(&state_path)?;

    // Create tmux session
    Command::new("tmux")
        .args(["new-session", "-d", "-s", "psyche"])
        .status()
        .expect("Failed to create tmux session");

    // Split windows and set up panes
    Command::new("tmux")
        .args(["split-window", "-h"])
        .status()
        .expect("Failed to split window horizontally");

    Command::new("tmux")
        .args(["select-pane", "-t", "0"])
        .status()
        .expect("Failed to select pane");

    Command::new("tmux")
        .args(["split-window", "-v"])
        .status()
        .expect("Failed to split window vertically");

    // Split remaining panes for clients
    Command::new("tmux")
        .args(["select-pane", "-t", "2"])
        .status()
        .expect("Failed to select pane");

    for _ in 1..args.num_clients {
        Command::new("tmux")
            .args(["split-window", "-v"])
            .status()
            .expect("Failed to split window for client");
    }

    // Start server
    let server_cmd = format!(
        "RUST_LOG={} cargo run -p psyche-centralized-server -- --state {} --data-config {} --server-port {}",
        args.log,
        state_path.display(),
        data_path.display(),
        args.server_port
    );

    println!("starting server: {server_cmd:?}");

    Command::new("tmux")
        .args(["select-pane", "-t", "0"])
        .status()
        .expect("Failed to select server pane");

    Command::new("tmux")
        .args(["send-keys", &server_cmd, "C-m"])
        .status()
        .expect("Failed to send server command");

    println!("Waiting for server startup...");
    loop {
        if TcpStream::connect(format!("127.0.0.1:{}", args.server_port)).is_ok() {
            println!("Server started!");
            break;
        }
    }

    // Start nvtop
    Command::new("tmux")
        .args(["select-pane", "-t", "1"])
        .status()
        .expect("Failed to select nvtop pane");

    Command::new("tmux")
        .args(["send-keys", "nvtop", "C-m"])
        .status()
        .expect("Failed to start nvtop");

    // Start clients
    for i in 2..=args.num_clients + 1 {
        start_client(&args, i, &run_id, true);
    }

    // Attach to tmux session
    let mut tmux_session = Command::new("tmux")
        .args(["attach-session", "-t", "psyche"])
        .spawn()?;

    if let Some(kill_num) = args.random_kill_num {
        std::thread::sleep(Duration::from_secs(args.random_kill_interval));
        let to_kill = {
            let mut client_nums: Vec<usize> = (2..=args.num_clients + 1).collect();

            client_nums.shuffle(&mut rand::thread_rng());

            client_nums.truncate(kill_num);
            client_nums
        };
        for kill in to_kill {
            Command::new("tmux")
                .args(["select-pane", "-t", &kill.to_string()])
                .status()
                .expect("Failed to select client pane");
            // send ctrl-c
            Command::new("tmux")
                .args(["send-keys", "-t", &kill.to_string(), "C-c"])
                .status()
                .expect("Failed to kill client");
            // restart client
            start_client(&args, kill, &run_id, false);
        }
    }

    let _ = tmux_session.wait(); // to prevent weird async tmux overlap with normal shell

    // failsafe kill
    Command::new("tmux")
        .args(["kill-session", "-t", "psyche"])
        .status()
        .expect("Failed to kill tmux session");

    Ok(())
}

fn start_client(args: &Args, i: usize, run_id: &String, print: bool) {
    let key_path = args
        .config_path
        .join("keys")
        .join(format!("client-{}.key", i - 1));

    Command::new("tmux")
        .args(["select-pane", "-t", &i.to_string()])
        .status()
        .expect("Failed to select client pane");

    let mut cmd: OsString = if let Some(token) = &args.hf_token {
        format!("HF_TOKEN={token} ").into()
    } else {
        OsString::new()
    };

    cmd.push(format!(
        "RUST_LOG={} RUST_BACKTRACE=1 cargo run -p psyche-centralized-client -- train --secret-key {} --run-id {} --server-addr localhost:{} --tui {}",
        args.log,
        key_path.display(),
        run_id,
        args.server_port,
        args.tui
    ));

    if let Some(dir) = &args.write_distro_data {
        cmd.push(" --write-gradients-dir ");
        cmd.push(dir);
    }

    if args.force_same_shuffle.contains(&(i - 1)) {
        cmd.push(" --fixed-batch-shuffle 0000000000000000000000000000000000000000000000000000000000000001");
    }

    if let Some(repo) = &args.first_client_checkpoint {
        if i == 2 {
            cmd.push(format!(" --checkpoint-dir ./checkpoints --hub-repo {repo}"));
        }
    }

    if args.write_log {
        std::fs::create_dir_all("./logs").unwrap();
        cmd.push(format!(" --write-log ./logs/client-{}.txt", i - 1))
    }

    if print {
        println!("starting client {i}: {cmd:?}");
    }

    Command::new("tmux")
        .args([OsString::from("send-keys"), cmd, OsString::from("C-m")])
        .status()
        .expect("Failed to send server command");
}
