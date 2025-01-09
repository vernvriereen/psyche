# Psyche Centralized Client

The Psyche Centralized Client is responsible for joining and participating in a training run, contributing to the model's training process, and sharing results with other peers. It is a CLI application with various configurable options.

## Installation

To install the client CLI, simply run:

```bash
just install
```

After installation, verify the available commands by running:

```bash
psyche-centralized-client --help
```

This will display the following usage information:

```plaintext
Usage: psyche-centralized-client <COMMAND>

Commands:
  show-identity  Display the client's identity
  train          Participate in a training run
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## Commands

The Psyche Centralized Client provides two primary commands: **show-identity** and **train**.

### **1. show-identity**

The `show-identity` command displays the client's unique identifier, used to participate in training runs.

#### Prerequisites

Before using this command, you need to generate a key file. *(TODO: Provide instructions for generating the key file)*.

#### Usage

Once the key file is ready, use its path to obtain your identity:

```bash
psyche-centralized-client show-identity --identity-secret-key-path <path_to_key_file>
```

*Example Output:* *(TODO: Add sample output)*

### **2. train**

The `train` command allows the client to join a training run and contribute to the model's training process.

#### Required Arguments

- **`run-id`**: A unique identifier for the training run. This ID allows the client to join a specific active run.
- **`server-addr`**: The address of the server hosting the training run.

#### Usage

```bash
psyche-centralized-client train --run-id <RUN_ID> --server-addr <SERVER_ADDR>
```

#### Optional Arguments

You can customize the client's behavior using additional optional arguments:

- **`identity-secret-key-path`**: Specifies the key file for client authentication.
- **`bind-p2p-port`**: Sets the port for the client's P2P network participation.
- **`tui`**: Enables a terminal-based graphical interface for monitoring analytics.
- **`data-parallelism`**: *(TODO: Provide details)*.
- **`tensor-parallelism`**: *(TODO: Provide details)*.
- **`micro-batch-size`**: *(TODO: Provide details)*.
- **`write-gradients-dir`**: Specifies the directory to store gradients from the training run.
- **`eval-tasks`**: Tasks for evaluating the model during training.
- **`eval-fewshots`**: *(TODO: Provide details)*.
- **`eval-seed`**: *(TODO: Provide details)*.
- **`eval-test-max-docs`**: *(TODO: Provide details)*.
- **`checkpoint-dir`**: *(TODO: Provide details)*.
- **`hub-repo`**: Path to the Hugging Face repository containing model data and configuration.
- **`write-log`**: *(TODO: Provide details)*.
- **`optim-stats-steps`**: *(TODO: Provide details)*.
- **`grad-accum-in-fp32`**: *(TODO: Provide details)*.
- **`dummy-training-delay-secs`**: *(TODO: Provide details)*.

For a detailed list of all available options, run:

```bash
psyche-centralized-client train --help
```

## WandB Integration

The client supports logging training results to **Weights & Biases (WandB)**. To enable this, provide the following arguments:

- **`wandb-project`**: Name of the WandB project.
- **`wandb-run`**: Run identifier for WandB.
- **`wandb-group`**: Group identifier for organizing multiple runs.
- **`wandb-entity`**: Name of the WandB entity.
