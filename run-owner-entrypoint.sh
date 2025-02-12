#! /bin/bash

solana config set --url http://psyche-solana-test-validator:8899
solana-keygen new --no-bip39-passphrase --force

solana airdrop 10 $(solana-keygen pubkey)

psyche-solana-client create-run --wallet-private-key-path "/root/.config/solana/id.json" --rpc "http://psyche-solana-test-validator:8899" --ws-rpc "ws://psyche-solana-test-validator:8900" --run-id "${RUN_ID}"
psyche-solana-client update-config --wallet-private-key-path "/root/.config/solana/id.json" --rpc "http://psyche-solana-test-validator:8899" --ws-rpc "ws://psyche-solana-test-validator:8900" --run-id "${RUN_ID}" --config-path "/usr/local/config.toml"
psyche-solana-client set-paused --wallet-private-key-path "/root/.config/solana/id.json" --rpc "http://psyche-solana-test-validator:8899" --ws-rpc "ws://psyche-solana-test-validator:8900" --run-id "${RUN_ID}" --resume
