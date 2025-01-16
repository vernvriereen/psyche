# Command-Line Help for `psyche-centralized-client`

This document contains the help content for the `psyche-centralized-client` command-line program.

**Command Overview:**

* [`psyche-centralized-client`↴](#psyche-centralized-client)
* [`psyche-centralized-client show-identity`↴](#psyche-centralized-client-show-identity)
* [`psyche-centralized-client train`↴](#psyche-centralized-client-train)

## `psyche-centralized-client`

**Usage:** `psyche-centralized-client <COMMAND>`

###### **Subcommands:**

* `show-identity` — Displays the client's unique identifier, used to participate in training runs
* `train` — Allows the client to join a training run and contribute to the model's training process



## `psyche-centralized-client show-identity`

Displays the client's unique identifier, used to participate in training runs

**Usage:** `psyche-centralized-client show-identity [OPTIONS]`

###### **Options:**

* `--identity-secret-key-path <IDENTITY_SECRET_KEY_PATH>` — Path to the clients secret key. Create a new random one running `openssl rand 32 > secret.key` or use the `RAW_IDENTITY_SECRET_KEY` environment variable



## `psyche-centralized-client train`

Allows the client to join a training run and contribute to the model's training process

**Usage:** `psyche-centralized-client train [OPTIONS] --run-id <RUN_ID> --server-addr <SERVER_ADDR>`

###### **Options:**

* `-i`, `--identity-secret-key-path <IDENTITY_SECRET_KEY_PATH>` — Path to the clients secret key. Create a new random one running `openssl rand 32 > secret.key`. If not provided a random one will be generated
* `-b`, `--bind-p2p-port <BIND_P2P_PORT>` — Sets the port for the client's P2P network participation. If not provided, a random port will be chosen
* `--tui <TUI>` — Enables a terminal-based graphical interface for monitoring analytics

  Default value: `true`

  Possible values: `true`, `false`

* `--run-id <RUN_ID>` — A unique identifier for the training run. This ID allows the client to join a specific active run
* `--server-addr <SERVER_ADDR>` — The address of the server hosting the training run
* `--data-parallelism <DATA_PARALLELISM>`

  Default value: `1`
* `--tensor-parallelism <TENSOR_PARALLELISM>`

  Default value: `1`
* `--micro-batch-size <MICRO_BATCH_SIZE>`
* `--write-gradients-dir <WRITE_GRADIENTS_DIR>` — If provided, every shared gradient this client sees will be written to this directory
* `--eval-tasks <EVAL_TASKS>`
* `--eval-fewshot <EVAL_FEWSHOT>`

  Default value: `0`
* `--eval-seed <EVAL_SEED>`

  Default value: `42`
* `--eval-task-max-docs <EVAL_TASK_MAX_DOCS>`
* `--checkpoint-dir <CHECKPOINT_DIR>` — If provided, every model parameters update will be save in this directory after each epoch
* `--hub-repo <HUB_REPO>` — Path to the Hugging Face repository containing model data and configuration
* `--wandb-project <WANDB_PROJECT>`
* `--wandb-run <WANDB_RUN>`
* `--wandb-group <WANDB_GROUP>`
* `--wandb-entity <WANDB_ENTITY>`
* `--write-log <WRITE_LOG>`
* `--optim-stats-steps <OPTIM_STATS_STEPS>`
* `--grad-accum-in-fp32`

  Default value: `false`
* `--dummy-training-delay-secs <DUMMY_TRAINING_DELAY_SECS>`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

