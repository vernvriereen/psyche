use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of clients
    #[clap(long)]
    #[arg(value_parser = validate_num_clients)]
    num_clients: u32,

    /// Config directory path
    #[arg(value_parser = validate_config_path)]
    #[clap(long)]
    config_path: PathBuf,

    /// Write DisTrO data to disk
    #[clap(long)]
    write_distro_data: Option<PathBuf>,

    /// Server port
    #[clap(long)]
    #[arg(default_value = "20000")]
    server_port: u16,

    /// Enable TUI
    #[clap(long)]
    #[arg(default_value = "true")]
    tui: String,

    /// Force listed clients to use the same random data shuffle, causing them to train on duplicate data.
    #[clap(long, value_delimiter = ',', default_values_t = &[])]
    force_same_shuffle: Vec<usize>,
}

fn validate_num_clients(s: &str) -> Result<u32> {
    let n: u32 = s
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
        "cargo run -p psyche-centralized-server -- --state {} --data-config {} --server-port {}",
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

    println!("Waiting 10 seconds for server startup...");
    sleep(Duration::from_secs(10));

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
    for i in 2..=args.num_clients as usize + 1 {
        let key_path = args
            .config_path
            .join("keys")
            .join(format!("client-{}.key", i - 1));

        Command::new("tmux")
            .args(["select-pane", "-t", &i.to_string()])
            .status()
            .expect("Failed to select client pane");

        let mut cmd: OsString = format!(
            "RUST_BACKTRACE=1 cargo run -p psyche-centralized-client -- train --secret-key {} --run-id {} --server-addr localhost:{} --tui {}",
            key_path.display(),
            run_id,
            args.server_port,
            args.tui
        ).into();

        if let Some(dir) = &args.write_distro_data {
            cmd.push(" --write-gradients-dir ");
            cmd.push(dir);
        }

        if args.force_same_shuffle.contains(&(i - 1)) {
            cmd.push(" --fixed-batch-shuffle 0000000000000000000000000000000000000000000000000000000000000001");
        }

        println!("starting client {i}: {cmd:?}");

        Command::new("tmux")
            .args([OsString::from("send-keys"), cmd, OsString::from("C-m")])
            .status()
            .expect("Failed to start client");
    }

    // Attach to tmux session
    Command::new("tmux")
        .args(["attach-session", "-t", "psyche"])
        .exec();
    Ok(())
}
