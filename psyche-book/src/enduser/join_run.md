# Joining a training run

## Pre-requisites

### NVIDIA Driver

For the moment, the Psyche client can only be operated in Linux systems. You will also need NVIDIA drivers support.
If your system does not have NVIDIA driver installed, make sure to follow the [official installation guide](https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/) for your Linux distribution.

### Docker Engine

In order to run the containerized Pysche client, you will need to install Docker Engine. Follow the [Docker Engine installation guide](https://docs.docker.com/engine/install/) for your Linux distribution.

### NVIDIA Container Toolkit

Required to enable GPU access inside Docker containers. This toolkit allows Docker to interface with the NVIDIA driver and runtime on the host system, needed by the Psyche client container to perform GPU computations for training models. Follow the [Nvidia Container Toolkit installation guide] for your Linux distribution.

## Solana RPC providers

To ensure reliability, performance, and security, all users must configure at least one private Solana RPC provider, though having two is recommended.
We recommend using a dedicated RPC service such as [Helius](https://www.helius.dev/), [QuickNode](https://www.quicknode.com/), [Triton](https://triton.one/), or self-hosting your own Solana RPC node.

## Setting environment file

A `.env` file should be created containing all the necessary configuration variables for joining a training run. These variables will be used to interact with the Solana blockchain, specify the model you'll contribute compute to, and configure the Psyche client based on your hardware resources.

The variables that should be set are the following:

- `RPC`: The RPC url of one of your private Solana provider.
- `WS_RPC`: The websocket url of the same private Solana provider.
- `RPC_2`: The RPC url of your other private Solana provider. If you don't have one, use a public alternative. For example, https://api.devnet.solana.com for Devnet.
- `WS_RPC_2`: The websocket url corresponding to the other private Solana provider, or the public counterpart if you don't have one. For example, wss://api.devnet.solana.com for Devnet.
- `RUN_ID`: The ID of the training run you will join.
- `NVIDIA_DRIVER_CAPABILITIES`: An environment variable used primarily by Docker containers that use the NVIDIA Container Toolkit. It is recommended to set it to 'all', `NVIDIA_DRIVER_CAPABILITIES=all`.
- `DATA_PARALELLISM`: The number of accelerators the training data will be distributed into. Set it to the number of GPUs avaiables in your machine.
- `MICRO_BATCH_SIZE`: Number of samples processed per GPU per step (affects memory usage, set it as high as VRAM allows)
- `AUTHORIZER`: The Solana address that delegated authorization to the Solana public key you will use to join the run. You can read more about [authorization here](./authentication.md)

## Running the Psyche client docker image

To download and run the psyche docker client run the following command, replacing `<path_to_env_file>` and
`<path_to_solana_pubkey>` with your own.

```bash
docker run -d \
    --env-file <path_to_env_file> \
    -e RAW_WALLET_PRIVATE_KEY="$(cat <path_to_solana_pubkey>)" \
    --gpus all \
    --network "host" \
    nousresearch/psyche-client:latest
```
