# Running a Local Testnet

The local testnet is just another application meant to use for testing and checking the behavior of the training, running a local coordinator and a couple clients that will go through some training rounds. We'll see how to setup the testnet and how to run it to train some test model with some test training data.

## Pre-requisites

Since we want to run many clients and the coordinator we'll need several terminal windows. The tool uses [tmux](https://github.com/tmux/tmux/wiki/Installing) to create them.

## Installation

You can build and check the local-testnet usage by running the following command:

```bash
cargo run -p psyche-local-testnet -- --help
```

This will display the following usage information:

```plaintext
Usage: psyche-local-testnet [OPTIONS] --num-clients <NUM_CLIENTS> --config-path <CONFIG_PATH>

Options:
      --num-clients <NUM_CLIENTS>
          Number of clients
      --config-path <CONFIG_PATH>
          Config directory path
      --write-distro-data <WRITE_DISTRO_DATA>
          Write DisTrO data to disk
      --server-port <SERVER_PORT>
          Server port [default: 20000]
      --tui [<TUI>]
          Enable TUI [env: TUI=] [default: true] [possible values: true, false]
      --random-kill-num <RANDOM_KILL_NUM>
          Kill N clients randomly every <RANDOM_KILL_INTERVAL> seconds
      --allowed-to-kill <ALLOWED_TO_KILL>
          Which clients we're allowed to kill randomly
      --random-kill-interval <RANDOM_KILL_INTERVAL>
          Kill <RANDOM_KILL_NUM> clients randomly every N seconds [default: 120]
      --log <LOG>
          [default: info,psyche=debug]
      --first-client-checkpoint <FIRST_CLIENT_CHECKPOINT>
          HF repo for the first client to checkpoint at
      --hf-token <HF_TOKEN>

      --write-log

      --wandb-project <WANDB_PROJECT>
          [env: WANDB_PROJECT=]
      --wandb-group <WANDB_GROUP>
          [env: WANDB_GROUP=]
      --wandb-entity <WANDB_ENTITY>
          [env: WANDB_ENTITY=]
      --optim-stats <OPTIM_STATS>
          [env: OPTIM_STATS=]
      --eval-tasks <EVAL_TASKS>
          [env: EVAL_TASKS=]
  -h, --help
          Print help
  -V, --version
          Print version
```

To get more information about the local-testnet usage and the different options that supports, check the generated docs on `psyche/docs/CommandLineHelp-local-testnet.md`
