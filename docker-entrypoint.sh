#! /bin/bash

psyche-solana-client train --wallet-private-key-path "/usr/local/id.json" --rpc "http://host.docker.internal:8899" --ws-rpc "ws://host.docker.internal:8900" --run-id "test" --micro-batch-size 8 --ticker --tui "false"
