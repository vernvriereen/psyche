#! /bin/bash

set -o errexit
set -e
set -m

cleanup() {
    echo -e "\nCleaning up background processes...\n"
    kill $(jobs -p) 2>/dev/null
    wait
}

trap cleanup INT EXIT

WALLET_FILE=${KEY_FILE:-"$HOME/.config/solana/id.json"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}

solana-test-validator -r 1>/dev/null &
echo -e "\n[+] Started test validator!"

sleep 3

pushd architectures/decentralized/solana-authorizer
anchor keys sync && anchor build --no-idl && anchor deploy --provider.cluster ${RPC} -- --max-len 500000
popd
echo -e "\n[+] Authorizer program deployed successfully!"

pushd architectures/decentralized/solana-coordinator
anchor keys sync && anchor build --no-idl && anchor deploy --provider.cluster ${RPC} -- --max-len 500000
popd
echo -e "\n[+] Coordinator program deployed successfully!"

sleep 10

echo -e "\n[+] Creating training run..."
cargo run --release --bin psyche-solana-client -- \
    create-run \
       --wallet-private-key-path ${WALLET_FILE} \
       --rpc ${RPC} \
       --ws-rpc ${WS_RPC} \
       --run-id ${RUN_ID}

echo -e "\n[+] Training run created successfully"

cargo run --release --bin psyche-solana-client -- \
    update-config \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --config-path ${CONFIG_FILE}

cargo run --release --bin psyche-solana-client -- \
    set-paused \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --resume

echo -e "\n[+] Testing Solana setup ready, starting Solana logs...\n"

solana logs --url ${RPC}
