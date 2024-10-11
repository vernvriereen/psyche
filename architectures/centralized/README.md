# Psyche Centralized Server and Client

## Local Testing

You can use `local.sh`, a Bash script that automates the process of launching a centralized server and multiple clients using tmux.

### Prerequisites

`nix develop` OR

- tmux
- nvtop (for GPU monitoring)

### Usage

```
./local.sh <NUM_CLIENTS> <CONFIG_PATH> <WRITE_DISTRO_DATA> [SERVER_PORT] [TUI]
```

- `NUM_CLIENTS`: Number of clients to launch
- `CONFIG_PATH`: Path to a folder containing `state.toml` and `data.toml`
- `WRITE_DISTRO_DATA`: directory to write gradient data, or "false" to disable
- `SERVER_PORT`: (Optional) port for the server (default: 20000)
- `TUI`: (Optional) enable/disable TUI for clients (default: true)

### Example

```bash
./local.sh 3 /../../config/llama2-20m-dolma-noverify ./distro-data/llama2-20m-noverify 20000 false
```

This command launches a server and 3 clients, using the configuration in `/path/to/config`, writing gradient data, using port 20000, and disabling the TUI for clients.
