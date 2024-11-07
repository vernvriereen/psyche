# Psyche Centralized Server and Client

## Local Testing

You can use the `local-testnet` binary in `/tools/rust-tools/local-testnet`, which automates the process of launching a centralized server and multiple clients using tmux.

### Prerequisites

`nix develop` OR

- tmux
- nvtop (for GPU monitoring)

### Usage

```
cargo run -p local-testnet -- --help
```

### Example Invocations

```bash
cargo run -p local-testnet -- --num-clients 3 --config-path ../../config/llama2-20m-dolma-noverify-no-checkpointer --write-distro-data ./distro-data/llama2-20m-noverify --tui false
```

This command launches a server and 3 clients, using the configuration in `/path/to/config`, writing gradient data, and disabling the TUI for clients.
