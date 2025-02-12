#! /bin/bash

solana config set --url http://psyche-solana-test-validator:8899
solana-keygen new --no-bip39-passphrase --force

solana airdrop 10 $(solana-keygen pubkey)
psyche-solana-client train --wallet-private-key-path "/root/.config/solana/id.json" --rpc "http://psyche-solana-test-validator:8899" --ws-rpc "ws://psyche-solana-test-validator:8900" --run-id ${RUN_ID} --ticker --tui "false"
