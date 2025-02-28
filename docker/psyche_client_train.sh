#! /bin/bash

set -o errexit

env_file="./config/client/.env"

if [[ ! -f "$env_file" ]]; then
    echo -e "\nEnvironment file does not exist. You must provide one."
    exit 1
fi

source "$env_file"

if [[ ! -f "$WALLET_FILE" ]]; then
    echo -e "\n[!] The file that was set in the WALLET_FILE env variable does not exist."
    exit 1
fi

if [[ "$RPC" == "" ]]; then
   echo -e "\n[!] The RPC env variable was not set."
   exit 1
fi

if [[ "$WS_RPC" == "" ]]; then
   echo -e "\n[!] The WS_RPC env variable was not set."
   exit 1
fi

if [[ "$RUN_ID" == "" ]]; then
   echo -e "\n[!] The RUN_ID env variable was not set."
   exit 1
fi

docker run -v "$WALLET_FILE":/keys/id.json \
    --gpus all \
    -e NVIDIA_DRIVER_CAPABILITIES=all \
    --name psyche-client \
    psyche-client train \
        --wallet-private-key-path "/keys/id.json" \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --ticker \
        --logs "json"
