#! /usr/bin/env bash

set -o errexit
set -e
set -m

RPC=${RPC:-"http://127.0.0.1:8899"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}
WALLET_FILE=${WALLET_FILE:-"$HOME/.config/solana/id.json"}

cleanup() {
    echo -e "\nCleaning up background processes...\n"
    kill $(jobs -p) 2>/dev/null
    wait
}

trap cleanup INT EXIT
solana-test-validator -r 1>/dev/null &
echo -e "\n[+] Started test validator!"

sleep 3

solana airdrop 10 --url ${RPC} --keypair ${WALLET_FILE}
CONFIG_FILE=${CONFIG_FILE} WALLET_FILE=${WALLET_FILE} ./scripts/deploy-solana-test.sh


echo -e "\n[+] Testing Solana setup ready, starting Solana logs...\n"

solana logs --url ${RPC}
