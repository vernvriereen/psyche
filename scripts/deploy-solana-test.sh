#!/usr/bin/env bash

set -o errexit
set -e
set -m

# use the agenix provided wallet if you have it
DEFAULT_WALLET=${devnet__keypair__wallet_PATH:-"$HOME/.config/solana/id.json"}
WALLET_FILE=${KEY_FILE:-"$DEFAULT_WALLET"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}

echo -e "\n[+] deploy info:"
echo -e "[+] WALLET_FILE = $WALLET_FILE"
echo -e "[+] RPC = $RPC"
echo -e "[+] WS_RPC = $WS_RPC"
echo -e "[+] RUN_ID = $RUN_ID"
echo -e "[+] CONFIG_FILE = $CONFIG_FILE"
echo -e "[+] -----------------------------------------------------------"

echo -e "\n[+] starting authorizor deploy"
pushd architectures/decentralized/solana-authorizer
    echo -e "\n[+] syncing keys..."
    anchor keys sync --provider.cluster ${RPC} --provider.wallet $WALLET_FILE

    echo -e "\n[+] building..."
    anchor build --no-idl

    echo -e "\n[+] deploying..."
    anchor deploy --provider.cluster ${RPC} --provider.wallet $WALLET_FILE -- --max-len 500000
popd
echo -e "\n[+] Authorizer program deployed successfully!"

echo -e "\n[+] starting coordinator deploy"
pushd architectures/decentralized/solana-coordinator
    echo -e "\n[+] syncing keys..."
    anchor keys sync --provider.cluster ${RPC} --provider.wallet $WALLET_FILE

    echo -e "\n[+] building..."
    anchor build --no-idl

    echo -e "\n[+] deploying..."
    anchor deploy --provider.cluster ${RPC} --provider.wallet $WALLET_FILE -- --max-len 500000
popd
echo -e "\n[+] Coordinator program deployed successfully!"

sleep 10

echo -e "\n[+] Creating training run..."
cargo run --release --bin psyche-solana-client -- \
    create-run \
       --wallet-private-key-path ${WALLET_FILE} \
       --rpc ${RPC} \
       --ws-rpc ${WS_RPC} \
       --run-id ${RUN_ID} "$@"

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
