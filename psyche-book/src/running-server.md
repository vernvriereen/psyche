# Psyche Centralized Server  

The Psyche Centralized Server is responsible for hosting the coordinator and a data provider locally to enable testing the network and training a test model. The server requires a configuration file named `state.toml` to load the initial settings for the coordinator.  

## Installation  

To install the client CLI, simply run:  

```bash
just install
```  

After installation, verify the available commands by running:  

```bash
psyche-centralized-server --help
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

## Commands  

The Psyche Centralized Server provides two primary commands: **validate-config** and **run**. Both commands require the initial state configuration, an example of which can be found in the `psyche/config` directory under `state.toml` files.  

### **1. validate-config**  

The `validate-config` command checks that the configuration declared in the `state.toml` file is valid.  

#### Usage  

```bash
psyche-centralized-server --state <PATH_TO_STATE_FILE> validate-config
```  

For example, running the following command from the root:  

```bash
psyche-centralized-server --state config/llama2-20m-dolma-noverify-no-checkpointer/state.toml validate-config
```  

will output:  

```bash
INFO psyche_centralized_server: Configs are OK!
```  

### **2. run**  

The `run` command starts the server and launches the coordinator with the declared configuration.  

#### Required Arguments  

- **`state`**: The path to the configuration file containing the initial settings for the coordinator.  

#### Usage  

```bash
psyche-centralized-server --state <PATH_TO_STATE_FILE> run
```  

#### Optional Arguments  

You can customize the server's behavior using additional optional arguments:  

- **`p2p-port`**: Specifies the port for the P2P network with the clients.  
- **`server-port`**: Specifies the port for the server, which clients will use to connect.  
- **`tui`**: Enables a terminal-based user interface for monitoring analytics.  
- **`data-config`**: Path to the TOML file containing data configurations (if the `data_location` in `state.toml` is set to `Server`, this is required).  
- **`save-state-dir`**: Path to save the server and coordinator state.  
- **`init-warmup-time`**: Sets the warmup time for the run.  
- **`init-min-clients`**: Sets the minimum number of clients required to start a run.  
- **`withdraw-on-disconnect`**: Allows clients to withdraw if they need to disconnect from the run (this option has no effect in the centralized version).  

For a detailed list of all available options, run:  

```bash
psyche-centralized-server run --help
```  
