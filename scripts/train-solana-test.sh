#! /bin/bash

set -e

WALLET_FILE=${KEY_FILE:-"$HOME/.config/solana/id.json"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}

# presets for a DGX or an HGX
DP=${DP:-"8"}
TP=${TP:-"1"}
BATCH_SIZE=${BATCH_SIZE:-"8"}

export RUST_LOG="warn,psyche_client=info,psyche_solana_client=info,psyche_network=info"

cargo run --release --bin psyche-solana-client -- \
    train \
        --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} --data-parallelism ${DP} --tensor-parallelism ${TP} --micro-batch-size ${BATCH_SIZE} --ticker