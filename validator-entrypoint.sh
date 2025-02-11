#! /bin/bash

cd /usr/local/solana-coordinator && anchor build --no-idl && anchor deploy --provider.cluster "solana-test-validator:8899" -- --max-len 500000
