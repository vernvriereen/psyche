# Psyche Centralized Server and Client

## Local Testing

You can use the `local-testnet` binary in `/tools/rust-tools/local-testnet`, which automates the process of launching a centralized server and multiple clients using tmux.

### Prerequisites

`nix develop` OR

- tmux
- nvtop (for GPU monitoring)

### Usage

```
cargo run -p local-testnet -- --num-clients <N> --config-path <CONFIG_PATH> [--write-distro-data <DIR>] [--server-port <PORT>] [--tui <bool>]
```

- `NUM_CLIENTS`: Number of clients to launch
- `CONFIG_PATH`: Path to a folder containing `state.toml` and `data.toml`
- `WRITE_DISTRO_DATA`: directory to write gradient data, or "false" to disable
- `SERVER_PORT`: (Optional) port for the server (default: 20000)
- `TUI`: (Optional) enable/disable TUI for clients (default: true)

### Example

```bash
cargo run -p local-testnet -- --num-clients 3 --config-path ../../config/llama2-20m-dolma-noverify-no-checkpointer --write-distro-data ./distro-data/llama2-20m-noverify --tui false
```

This command launches a server and 3 clients, using the configuration in `/path/to/config`, writing gradient data, and disabling the TUI for clients.
