#! /bin/bash

set -e

WALLET_FILE=${KEY_FILE:-"$HOME/.config/solana/id.json"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
DP=${DP:-"8"}
TP=${TP:-"1"}

export RUST_LOG="error,psyche_client=info,psyche_solana_client=info,psyche_network=info"

cargo run --release --bin psyche-solana-client -- \
    train \
        --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} --data-parallelism ${DP} --tensor-parallelism ${TP}