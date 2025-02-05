#! /bin/bash

psyche-solana-client train --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} --run-id ${RUN_ID} --ticker --tui "false"
