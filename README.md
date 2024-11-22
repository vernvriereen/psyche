# psyche

Psyche uses `just` to run some common tasks!
It uses `nix` as a build system, to make your life easier.
To install `nix`, simply run `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install` or find it at your local package manager.

## Setup

### Non-Nix

1. Download & install the latest Rust version (probably using `rustup`: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
2. Download `libtorch` 2.4.1 with CUDA 12.4 from this link: https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
3. Extract it to some folder with `tar -zxvf libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip`
4. Modify your environment variables (bashrc, zshrc, etc) to include `export LIBTORCH=/path/to/libtorch`, pointing to the libtorch folder you just extracted
5. (optional) Install `just`

### Nix

#### Direnv

0. Install `direnv`
1. `direnv allow`

#### Non-direnv

`nix develop` to enter a development shell

## Lints & Checks

`$ just check`

If it passes, CI will pass.

## Formatting

`$ just fmt`

## Building

You can build individual binaries with commands like

```bash
nix build .#psyche-centralized-client
nix build .#psyche-centralized-server
nix build .#expand-distro
```

## Building & pushing Docker images

To build the centralized client & push it to docker.io's hub,
`$ just docker-push-centralized-client`

## Utils

### compare-hf-psyche.sh

compares hf & psyche training implementations bit-for-bit.

## Notes

Running a Psyche client may require setting `NCCL_P2P_DISABLE=1` -- in a Dockerized environment single-process NCCL deadlocks (but works in bare metal).
