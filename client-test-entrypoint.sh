#! /bin/bash

solana config set --url solana-text-validator:8899
solana-keygen new --no-bip39-passphrase
solana airdrop 1 $(solana-keygen pubkey ~/.config/solana/id.json)

psyche-solana-client train --wallet-private-key-path "/root/.config/solana/id.json" --rpc "solana-test-validator:8899" --ws-rpc "solana-test-validator:8900" --run-id ${RUN_ID} --ticker --tui "false"
