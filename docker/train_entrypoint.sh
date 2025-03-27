#! /bin/bash

set -o errexit

if [[ "$RAW_WALLET_PRIVATE_KEY" == "" ]]; then
    echo -e "\n[!] The RAW_WALLET_PRIVATE_KEY env variable was not set."
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

echo "[+] Starting to train in run '${RUN_ID}'..."

/usr/local/bin/psyche-solana-client train \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --ticker \
    --logs "console"

echo "Training ended"
exit 0
