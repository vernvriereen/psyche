#! /bin/bash

set -o errexit

WALLET_FILE=${KEY_FILE:-"$HOME/.config/solana/id.json"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}

solana airdrop 10 "$(solana-keygen pubkey)" --url "${RPC}"

psyche-solana-client create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}"

psyche-solana-client update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --config-path "/usr/local/config.toml"

psyche-solana-client set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --resume
