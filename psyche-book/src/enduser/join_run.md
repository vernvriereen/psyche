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

## Setting environment file

## Running docker image

```bash
docker run --detach --env-file ./.env --gpus all --network "host" -e RAW_WALLET_PRIVATE_KEY="$(cat ./plaintext/devnet_funded_accounts/keypair_1.json)" nousresearch/psyche-client:latest
```
