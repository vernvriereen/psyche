#!/usr/bin/env bash

set -euox

# Check if the required argument is provided
if [ $# -ne 1 ]; then
    echo "Usage: $0 <NUM_CLIENTS>"
    exit 1
fi

NUM_CLIENTS=$1

# Check if NUM_CLIENTS is a positive integer
if ! [[ "$NUM_CLIENTS" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: NUM_CLIENTS must be a positive integer"
    exit 1
fi

# Create a new tmux session
tmux new-session -d -s bandwidth-test

# Split panes for each client
for ((i=1; i<NUM_CLIENTS; i++)); do
    tmux split-window -v
done

# Start the first client and get its node ID
tmux select-pane -t 0
tmux send-keys "cargo run --example bandwidth_test -- --tui false" C-m

# Wait for and extract the node ID
echo "Waiting for first node to start..."
node_id=""
while [ -z "$node_id" ]; do
    if ! tmux has-session -t bandwidth-test 2>/dev/null; then
        echo "Tmux session ended unexpectedly"
        exit 1
    fi
    node_id=$(tmux capture-pane -J -S- -E- -p -t bandwidth-test.0 | grep -o ".*our join ticket: \w\+" | tail -n1 | sed 's/.*our join ticket: \(\w\+\)/\1/')
    echo "Found node ID: $node_id"
    echo $node_id
    sleep 1
done
echo "Found node ID: $node_id"

# Start the other clients
for ((i=1; i<NUM_CLIENTS; i++)); do
    tmux select-pane -t $i
    tmux send-keys "cargo run --example bandwidth_test -- $node_id" C-m
done

# Attach to the tmux session
exec tmux attach-session -t bandwidth-test