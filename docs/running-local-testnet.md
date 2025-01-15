# Running a Local Testnet

The local testnet is just another application meant to use for testing and checking the behavior of the training, running a local coordinator and a couple clients that will go through some training rounds. We'll see how to setup the testnet and how to run it to train some test model with some test training data.

## Pre-requisites

Since we want to run many clients and the coordinator we'll need several terminal windows. The tool uses [tmux](https://github.com/tmux/tmux/wiki/Installing) to create them.

## Installation

To install the CLI application, simply run:

```bash
just install
```

After installation, verify the version to check that everything went well

```bash
psyche-local-testnet --version # 0.0.1
```

We can also see more information about the tool running

```bash
psyche-local-testnet --help
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

## Usage

First we'll need the training data that we'll use for this run. (TODO: check how to obtain real data).

### Required arguments

The main things the local testnet need to start are two things:
- `num-clients` This is the number of clients that will be participating in the run
- `config-path` This is the file path to the configuration that the coordinator will need to start. You can find some configuration examples in `psyche/config/` where the `data.toml` and the `state.toml` files are located for different run setups. In the coordinator section we'll talk more about all this different configurations.

To run a local-testnet do as follows

```bash
psyche-local-testnet --num-clients <NUM_CLIENTS> --config-path <PATH_TO_CONFIG_FILE>
```

That will start a tmux session with many windows containing each of the actors that will be present in the run going through several training runs and training a real test model.

### Optional arguments

You can customize your local testnet using the additional optional arguments:
- `write-distro-data` this arguments receives a file path where extra data for the optimizer and the run can be stored.
- `server-port` we can specify the port where the server for this testnet will be listen it to (this is the ones that clients must use when connecting).
- `tui` Enables a terminal-based graphical interface for monitoring analytics.
- `random-kill-num` receives a time interval and an amount of clients, if present the testnet will kill N clients every M interval pass.
- `allowed-to-kill` determines which clients we're allowed to kill randomly
- `log` can set the level of the logging for more granular information
- `first-client-checkpoint` receives a hugging face repo where the first client could get the model and the configuration to use
- `hf-token` the hugging face token for all the clients to fetch the model at first.
- `write-log` TODO
- `optim-stats` TODO
- `eval-tasks`: Tasks for evaluating the model during training.

## WandB Integration

The testnet supports logging training results to **Weights & Biases (WandB)**. To enable this, provide the following arguments:

- `wandb-project` Name of the WandB project.
- `wandb-stats` Stats for wandb.
- `wandb-group`: Group identifier for organizing multiple runs.
- `wandb-entity` Name of the WandB entity.

