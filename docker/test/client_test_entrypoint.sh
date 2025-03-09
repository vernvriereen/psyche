#!/bin/bash

set -o errexit

solana airdrop 10 "$(solana-keygen pubkey)" --url "${RPC}"

psyche-solana-client train \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --ticker \
    --logs "json"
