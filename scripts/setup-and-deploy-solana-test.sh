#! /usr/bin/env bash

set -o errexit
set -e
set -m

RPC=${RPC:-"http://127.0.0.1:8899"}

cleanup() {
    echo -e "\nCleaning up background processes...\n"
    kill $(jobs -p) 2>/dev/null
    wait
}

trap cleanup INT EXIT

solana-keygen new --no-bip39-passphrase
solana config set --url localhost
solana-test-validator -r 1>/dev/null &
echo -e "\n[+] Started test validator!"

sleep 3

./deploy-solana-test.sh


echo -e "\n[+] Testing Solana setup ready, starting Solana logs...\n"

solana logs --url ${RPC}