#!/bin/bash

set -o errexit
set -m

RPC=${RPC:-"http://localhost:8899"}

solana-keygen new --no-bip39-passphrase --force
solana-keygen new --no-bip39-passphrase --outfile /solana_keys/client1_key.json --force
solana-keygen new --no-bip39-passphrase --outfile /solana_keys/client2_key.json --force
solana-keygen new --no-bip39-passphrase --outfile /solana_keys/client3_key.json --force

solana config set --url localhost
solana-test-validator -r &

sleep 5

solana airdrop 5 $(solana-keygen pubkey /solana_keys/client1_key.json)
solana airdrop 5 $(solana-keygen pubkey /solana_keys/client2_key.json)
solana airdrop 5 $(solana-keygen pubkey /solana_keys/client3_key.json)

pushd architectures/decentralized/solana-coordinator
anchor deploy --provider.cluster ${RPC} -- --max-len 500000
popd

fg %1
