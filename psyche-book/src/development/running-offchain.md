# Running Psyche offchain

When developing for Psyche, you might not want to spin up all the Solana infrastructure if you're working on a feature like the distributed networking or the training code.

To that end, we maintain a "centralized" client & server package that simply communicate over TCP instead of dealing with code deployed to a Solana network.

There's a `server` package, and a `client` package.
To develop with them, you'd spin up one `server` with whatever [run config](../enduser/run-config.md) you want

## Local Testnet

The local testnet is a helper application designed to easily spin up a Server and multiple clients.
It's useful for doing sample runs on your own hardware, and for development.

### Pre-requisites

Since we want to run many clients and the server we'll need several terminal windows to monitor them. The tool uses [tmux](https://github.com/tmux/tmux/wiki/Installing) to create them.

> If you're using the Nix devShell, tmux is already included.

### Running

A sample invocation that fires up 3 clients to train on a 20m model might look like this:

```bash
just local-testnet \
    --num-clients 3 \
    --config-path ./config/consilience-match-llama2-20m-fineweb-pretrain-dev/
```

There's a _lot_ of options to configure the local testnet. Check em out below!

<details>
    <summary>Command-line options</summary>
    {{#include ../../generated/cli/psyche-centralized-local-testnet.md}}
</details>

## Server & Client

Both of these applications can be spun up individually at your discretion instead of using the local testnet. We include all their command-line options for your reading pleasure:

<details>
    <summary>Client</summary>
    {{#include ../../generated/cli/psyche-centralized-client.md}}
</details>

<details>
    <summary>Server</summary>
    {{#include ../../generated/cli/psyche-centralized-server.md}}
</details>
