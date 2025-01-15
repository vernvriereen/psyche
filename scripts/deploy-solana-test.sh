#! /bin/bash

WALLET_FILE=${KEY_FILE:-"$HOME/.config/solana/id.json"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}

pushd architectures/decentralized/solana-coordinator
anchor build --no-idl && anchor deploy
popd

cargo run --release --bin psyche-solana-client -- \
    create-run \
       --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} \
       --run-id ${RUN_ID}
cargo run --release --bin psyche-solana-client -- \
    update-config \
        --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} --config-path ${CONFIG_FILE}
cargo run --release --bin psyche-solana-client -- \
    set-paused \
        --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} --config-path ${CONFIG_FILE} --resume