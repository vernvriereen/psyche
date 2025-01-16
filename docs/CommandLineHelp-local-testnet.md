# Command-Line Help for `psyche-local-testnet`

This document contains the help content for the `psyche-local-testnet` command-line program.

**Command Overview:**

* [`psyche-local-testnet`↴](#psyche-local-testnet)
* [`psyche-local-testnet start`↴](#psyche-local-testnet-start)

## `psyche-local-testnet`

**Usage:** `psyche-local-testnet <COMMAND>`

###### **Subcommands:**

* `start` — Starts the local-testnet running each part of the system in a separate terminal pane



## `psyche-local-testnet start`

Starts the local-testnet running each part of the system in a separate terminal pane

**Usage:** `psyche-local-testnet start [OPTIONS] --num-clients <NUM_CLIENTS> --config-path <CONFIG_PATH>`

###### **Options:**

* `--num-clients <NUM_CLIENTS>` — Number of clients to start
* `--config-path <CONFIG_PATH>` — File path to the configuration that the coordinator will need to start
* `--write-distro-data <WRITE_DISTRO_DATA>` — If provided, write DisTrO data to disk in this path
* `--server-port <SERVER_PORT>` — Port where the server for this testnet will be listen it to (this is the one that clients must use when connecting)

  Default value: `20000`
* `--tui <TUI>` — Enables a terminal-based graphical interface for monitoring analytics

  Default value: `true`

  Possible values: `true`, `false`

* `--random-kill-num <RANDOM_KILL_NUM>` — Kill N clients randomly every <RANDOM_KILL_INTERVAL> seconds
* `--allowed-to-kill <ALLOWED_TO_KILL>` — Which clients we're allowed to kill randomly
* `--random-kill-interval <RANDOM_KILL_INTERVAL>` — Kill <RANDOM_KILL_NUM> clients randomly every N seconds

  Default value: `120`
* `--log <LOG>` — Sets the level of the logging for more granular information

  Default value: `info,psyche=debug`
* `--first-client-checkpoint <FIRST_CLIENT_CHECKPOINT>` — HF repo where the first client could get the model and the configuration to use
* `--hf-token <HF_TOKEN>`
* `--write-log`

  Default value: `false`
* `--wandb-project <WANDB_PROJECT>`
* `--wandb-group <WANDB_GROUP>`
* `--wandb-entity <WANDB_ENTITY>`
* `--optim-stats <OPTIM_STATS>`
* `--eval-tasks <EVAL_TASKS>`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

