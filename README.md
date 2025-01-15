# psyche

Psyche uses `just` to run some common tasks!
It uses `nix` as a build system, to make your life easier.
To install `nix`, simply run `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install` or find it at your local package manager.

## Setup

### Ubuntu

The following instructions are needed for a server with a fresh Ubuntu installation

1. Install drivers

```bash
sudo apt update
sudo apt install -y ubuntu-drivers-common
sudo ubuntu-drivers install
```

2. Install CUDA libraries

```bash
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt-get update
sudo apt-get -y install cuda-toolkit-12-4
rm cuda-keyring_1.1-1_all.deb
sudo apt-get install libnccl-dev libnccl2
sudo apt install nvidia-cuda-toolkit
```

3. Download libtorch & extract

```bash
wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
unzip libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
rm libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
```

4. In the `.bashrc` file, set the following libtorch environment variables. Here `<path_to_libtorch>` is the absolute path
to the extracted `libtorch` folder from the previous step

```bash
export LIBTORCH=<path_to_libtorch>
export LIBTORCH_INCLUDE=<path_to_libtorch>
export LIBTORCH_LIB=<path_to_libtorch>
export LD_LIBRARY_PATH=<path_to_libtorch>/lib:$LD_LIBRARY_PATH
```

This can also be acheived by making a `.cargo/config.toml` file in the checkout path

```
[env]
LIBTORCH=<path_to_libtorch>
LD_LIBRARY_PATH=<path_to_libtorch>/lib
CUDA_ROOT = "/usr/local/cuda-12.4"
```

5. Download & install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

6. (optional) Install `just`

```bash
sudo snap install just --edge --classic
```

7. (optional) Install Solana and Anchor

Install Solana
```bash
sh -c "$(curl -sSfL https://release.anza.xyz/beta/install)"
```

After installation, follow the instructions to add the Solana tools to PATH.

Install Anchor
```bash
cargo install --git https://github.com/coral-xyz/anchor --rev a7a23eea308440a9fa9cb79cee7bddd30ab163d5 anchor-cli
```

### Nix

#### Direnv

0. Install `direnv`
1. `direnv allow`

#### Non-direnv

`nix develop` to enter a development shell

### Windows

1. Install CUDA libraries: https://developer.nvidia.com/cuda-12-4-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11

2. Download libtorch & extract: https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip

3. Download OpenSSL: https://slproweb.com/download/Win64OpenSSL-3_3_2.exe

4. Install Perl: https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit.msi

5. Create a `.cargo/config.toml` file to set environment variables

**NOTE**: Building may take several minutes the first time as `openssl-sys` takes a long time (for some reason)

```
[env]
LIBTORCH = <path_to_libtorch>
OPENSSL_LIB_DIR = <path_to_openssl>/lib/VC/x64/MT
OPENSSL_INCLUDE_DIR <path_to_openssl>/include
```

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

To build the centralized client & push it to docker.io's hub, `just docker-push-centralized-client`

## Solana

To build the Solana programs, install required Solana tools (Step 7 in Setup).
For local development, create a wallet and switch the using a local validator.

```bash
solana-keygen new
solana config set --url localhost
```
Then, in a new terminal, run a validator with `solana-test-validator`.
Once that's running, you can deploy the solana coordinator using `just deploy-local-solana-coordinator`
and send a basic transaction from a client running `cargo run -p psyche-solana-client`

## Build and Serve the Psyche Book  

To install the necessary dependencies, run:  

```bash
just book_deps
```  

To serve the book locally, use:  

```bash
just serve_book
```  

If you prefer to manually generate the HTML files to serve with another tool, run:  

```bash
just build_book <OUTPUT_DIR>
```  

Where `OUTPUT_DIR` specifies the directory where the book will be built. If no directory is provided, the root of the `Psyche` repository will be used by default.

## Utils

### compare-hf-psyche.sh

compares hf & psyche training implementations bit-for-bit.

## Notes

Running a Psyche client may require setting `NCCL_P2P_DISABLE=1` -- in a Dockerized environment single-process NCCL deadlocks (but works in bare metal).
