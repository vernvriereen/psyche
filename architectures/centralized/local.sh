
#!/usr/bin/env bash

set -euo pipefail

# Check if the required arguments are provided
if [ $# -lt 3 ] || [ $# -gt 5 ]; then
    echo "Usage: $0 <NUM_CLIENTS> <CONFIG_PATH> <WRITE_DISTRO_DATA> [SERVER_PORT] [TUI]"
    exit 1
fi

# Parse arguments
NUM_CLIENTS=$1
STATE_PATH="${2%/}/state.toml"
DATA_PATH="${2%/}/data.toml"
WRITE_DISTRO_DATA=$3
SERVER_PORT=${4:-20000}  # Default to 20000 if not provided
TUI=${5:-true}  # Default to true if not provided

# Check if NUM_CLIENTS is a positive integer
if ! [[ "$NUM_CLIENTS" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: NUM_CLIENTS must be a positive integer"
    exit 1
fi

# Extract run_id from STATE_PATH
run_id=$(grep 'run_id = ' "$STATE_PATH" | sed 's/run_id = "\(.*\)"/\1/')

if [ -z "$run_id" ]; then
    echo "Error: Could not extract run_id from $STATE_PATH"
    exit 1
fi

# Pre-build the packages
cargo build -p psyche-centralized-server
cargo build -p psyche-centralized-client

cargo run -p psyche-centralized-server -- --state $STATE_PATH --data-config $DATA_PATH validate-config

# Create a new tmux session
tmux new-session -d -s psyche

# Split the first pane horizontally for the server
tmux split-window -h

# Split the server pane vertically for nvtop
tmux select-pane -t 0
tmux split-window -v

# Split the remaining panes vertically for clients
tmux select-pane -t 2
for ((i=1; i<NUM_CLIENTS; i++)); do
    tmux split-window -v
done

# Select the first pane (server pane)
tmux select-pane -t 0

# Send the server command to the first pane
tmux send-keys "cargo run -p psyche-centralized-server -- --state $STATE_PATH --data-config $DATA_PATH --server-port $SERVER_PORT" C-m
# Wait a sec for startup..
echo "Starting server & waiting 10 seconds for server startup..."
sleep 10

# Select the second pane (nvtop pane)
tmux select-pane -t 1

# Send the nvtop command to the second pane
tmux send-keys "nvtop" C-m


# Send client commands to the rest of the panes
for ((i=2; i<=(NUM_CLIENTS+1); i++)); do
    key_path="${2%/}/keys/client-$((i-1)).key"

    tmux select-pane -t $i

    cmd="RUST_BACKTRACE=1 cargo run -p psyche-centralized-client -- train --secret-key $key_path --run-id $run_id --server-addr localhost:$SERVER_PORT --tui $TUI"
    if [ "$WRITE_DISTRO_DATA" != "false" ]; then
        cmd="$cmd --write-gradients-dir $WRITE_DISTRO_DATA"
    fi

    if [ $i -eq 2 ]; then
        cmd="$cmd --hub-repo bug-free-chainsaw/tiny-local-20m --checkpoint-dir ../../checkpoints"
    fi
    tmux send-keys "$cmd" C-m
done

# Attach to the tmux session
tmux attach-session -t psyche