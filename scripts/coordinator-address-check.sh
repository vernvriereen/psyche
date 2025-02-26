#!/bin/bash

set -o errexit
set -o pipefail

cd architectures/decentralized/solana-coordinator
coordinator_address=$(grep -r --include='*.rs' 'declare_id!(' . | sed -n 's/.*declare_id!(\([^)]*\)).*/\1/p')

if [ -z "$coordinator_address" ]; then
  echo "Error: No declare_id! macro found."
  exit 1
fi

echo -e "\n[+] ${coordinator_address} is the address of the coordinator program that will be used in the psyche test client binary.\n"
read -p "Continue? [y/N] " answer

if [[ "$answer" =~ ^[Yy]$ || -z "$answer" ]]; then
    echo "Continuing..."
else
    echo "Exiting."
    exit 1
fi
