# Psyche Centralized Server  

The Psyche Centralized Server is responsible for hosting the coordinator and a data provider locally to enable testing the network and training a test model. The server requires a configuration file named `state.toml` to load the initial settings for the coordinator.  

## Installation  

You can build and check the server usage by running the following command:

```bash
cargo run -p psyche-centralized-server -- --help
```

This will display the following usage information:  

```plaintext
Usage: psyche-centralized-server [OPTIONS] --state <STATE> <COMMAND>

Commands:
  validate-config
  run
  help             Print this message or the help of the given subcommand(s)

Options:
      --state <STATE>
          Path to TOML of Coordinator state
  -p, --p2p-port <P2P_PORT>
          If not specified, a random free port will be chosen
  -s, --server-port <SERVER_PORT>
          If not specified, a random free port will be chosen
      --tui [<TUI>]
          [default: true] [possible values: true, false]
      --data-config <DATA_CONFIG>
          Path to TOML of data server config
      --save-state-dir <SAVE_STATE_DIR>

      --init-warmup-time <INIT_WARMUP_TIME>

      --init-min-clients <INIT_MIN_CLIENTS>

      --withdraw-on-disconnect [<WITHDRAW_ON_DISCONNECT>]
          [default: true] [possible values: true, false]
  -h, --help
          Print help
```

To get more information about the server usage and the different options that supports, check the generated docs on `psyche/docs/CommandLineHelp-server.md`
