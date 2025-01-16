# Psyche Centralized Client

The Psyche Centralized Client is responsible for joining and participating in a training run, contributing to the model's training process, and sharing results with other peers. It is a CLI application with various configurable options.

## Installation

You can build and check the client usage by running the following command:

```bash
cargo run -p psyche-centralized-client -- --help
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

To get more information about the client usage and the different options that supports, check the generated docs on `psyche/docs/CommandLineHelp-client.md`
