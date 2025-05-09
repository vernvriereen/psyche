#!/bin/bash

# ---------------------------------------------------------- #
# This script should be run from the root of the Psyche repo #
# ---------------------------------------------------------- #

set -o errexit
set -o pipefail

funded_accounts_folder="./devnet_funded_accounts"

num_clients=$1

source ./config/client/.env

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

if [[ ! -d "$funded_accounts_folder" ]]; then
    echo -e "\nFunded accounts folder does not exist. To fund accounts, run the 'fund_accounts.sh' script."
    exit 1
fi

if [[ "$num_clients" == "" ]]; then
    echo -e "[!] No number of clients set. Exiting"
    exit 1
fi

num_wallets=$(($(ls -1 "$funded_accounts_folder" | wc -l) - 1))
if [ "$num_clients" -gt "$num_wallets" ]; then
    echo -e "\n[!] There are not enough wallets for the desired number of clients"
    echo -e "Number of wallets: $num_wallets - Number of desired clients: $num_clients"
    exit 1
fi

# Get the number of available GPUs
num_gpus=$(nvidia-smi --query-gpu=index --format=csv,noheader | wc -l)
if [ "$num_clients" -gt "$num_gpus" ]; then
    echo -e "\n[!] There are not enough GPUs for the desired number of clients"
    echo -e "Number of GPUs: $num_gpus - Number of desired clients: $num_clients"
    exit 1
fi

for i in $(seq 1 "$num_clients"); do
    gpu_id=$((i - 1))

    if [ "$gpu_id" -ge "$num_gpus" ]; then
        echo "Error: GPU index $gpu_id does not exist. Exiting." >&2
        exit 1
    fi

    if docker ps -a --format '{{.Names}}' | grep -q "^psyche-client-${i}$"; then
        docker rm -f psyche-client-"${i}"
    fi

    docker run --rm -d -v "$funded_accounts_folder"/keypair_"${i}.json":/keys/id.json \
        --name psyche-client-"${i}" \
        --gpus "device=$gpu_id" \
        -e NVIDIA_DRIVER_CAPABILITIES=all \
        --add-host=host.docker.internal:host-gateway \
        psyche-client train \
        --wallet-private-key-path "/keys/id.json" \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --logs "json"

    echo "Started psyche-client-${i} on GPU $gpu_id"
done
