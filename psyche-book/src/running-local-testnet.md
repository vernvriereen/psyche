# Running a Local Testnet

The local testnet is a helper application designed to easily spin up a coordinator and multiple clients.
It's useful for doing sample runs on your own hardware, and for development.

## Pre-requisites

Since we want to run many clients and the coordinator we'll need several terminal windows to monitor them. The tool uses [tmux](https://github.com/tmux/tmux/wiki/Installing) to create them.

> If you're using the Nix flake, tmux is already included.

{{#include ./cli/centralized-local-testnet.md}}
