# Command-Line Help for `psyche-centralized-server`

This document contains the help content for the `psyche-centralized-server` command-line program.

**Command Overview:**

* [`psyche-centralized-server`↴](#psyche-centralized-server)
* [`psyche-centralized-server validate-config`↴](#psyche-centralized-server-validate-config)
* [`psyche-centralized-server run`↴](#psyche-centralized-server-run)

## `psyche-centralized-server`

**Usage:** `psyche-centralized-server <COMMAND>`

###### **Subcommands:**

* `validate-config` — Checks that the configuration declared in the `state.toml` file is valid
* `run` — Starts the server and launches the coordinator with the declared configuration



## `psyche-centralized-server validate-config`

Checks that the configuration declared in the `state.toml` file is valid

**Usage:** `psyche-centralized-server validate-config [OPTIONS] <STATE>`

###### **Arguments:**

* `<STATE>` — Path to the `state.toml` file to validate

###### **Options:**

* `--data-config <DATA_CONFIG>` — Path to `data.toml` file to validate. If no provided then it will not be checked



## `psyche-centralized-server run`

Starts the server and launches the coordinator with the declared configuration

**Usage:** `psyche-centralized-server run [OPTIONS] --state <STATE>`

###### **Options:**

* `--state <STATE>` — Path to TOML of Coordinator state
* `-s`, `--server-port <SERVER_PORT>` — Port for the server, which clients will use to connect. if not specified, a random free port will be chosen
* `--tui <TUI>`

  Default value: `true`

  Possible values: `true`, `false`

* `--data-config <DATA_CONFIG>` — Path to TOML of data server config
* `--save-state-dir <SAVE_STATE_DIR>` — Path to save the server and coordinator state
* `--init-warmup-time <INIT_WARMUP_TIME>` — Sets the warmup time for the run. This overrides the `warmup_time` declared in the state file
* `--init-min-clients <INIT_MIN_CLIENTS>` — Sets the minimum number of clients required to start a run. This overrides the `min_clients` declared in the state file
* `--withdraw-on-disconnect <WITHDRAW_ON_DISCONNECT>` — Allows clients to withdraw if they need to disconnect from the run (this option has no effect in the centralized version)

  Default value: `true`

  Possible values: `true`, `false`




<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

