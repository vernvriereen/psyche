#! /bin/bash

psyche-solana-client train \
    --wallet-private-key-path "/usr/local/id.json" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --ticker \
    --logs "json"
