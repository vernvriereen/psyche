#! /bin/bash

solana-keygen new --no-bip39-passphrase

solana-test-validator -r &
SOLANA_PID=$!

sleep 5
cd /usr/local/solana-coordinator && anchor deploy --provider.cluster "localnet" -- --max-len 500000

wait $SOLANA_PID
