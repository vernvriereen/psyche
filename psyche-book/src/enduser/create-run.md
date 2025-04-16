# Creating a run

To create a new training run and make it available for nodes to join, you'll need to create it, configure it, and unpause it.

First, create the run on-chain.
You'll need to provide:

- the RPC & websocket RPC urls so the client can communicate with an RPC node.
- a unique run ID - just a few characters to uniquely identify your run.
- a name & description for your run

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --name [NAME] \
    --description [DESCRIPTION]
```

Then, set the run's config.
You'll need to provide:

- the RPC & websocket RPC urls so the client can communicate with an RPC node.
- the run ID you previously used
- the path to a `config.toml` file, following the [run config schema](./run-config.md)

```bash
psyche-solana-client update-config \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE]
```

At this point, your run is ready to go! You can now set its state to "unpaused", and let clients join & begin training your model.

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    resume
```

Congratulations! As soon as your first client joins, your model is being trained.
