# Setup & Useful Commands

## Installation and Setup

### Any Linux, via Nix

Psyche can use `nix` + flakes as a build system, to make your life easier.
To install `nix`, simply run `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install` or find it at your local package manager.

You can optionally use `direnv` to automatically enter a Nix environment when you `cd` into the Psyche folder.
Either option will install every single dependency and development tool Psyche needs to run and be developed.

#### Using `direnv`

Install `direnv` from your system's package manager.
After running `direnv allow` in the Psyche directory once, your terminal will automatically enter a development shell when you subsequently `cd` into the Psyche directory.

#### Without `direnv`

Enter the Psyche directory, then run `nix develop` to enter a development shell.

### Ubuntu

The following instructions are needed for a server with a fresh Ubuntu installation

#### 1. Install drivers

```bash
sudo apt update
sudo apt install -y ubuntu-drivers-common
sudo ubuntu-drivers install
```

#### 2. Install CUDA libraries

```bash
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt-get update
sudo apt-get -y install cuda-toolkit-12-4
rm cuda-keyring_1.1-1_all.deb
sudo apt-get install libnccl-dev libnccl2
sudo apt install nvidia-cuda-toolkit
```

#### 3. Download libtorch & extract

```bash
wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.6.0%2Bcu124.zip
unzip libtorch-cxx11-abi-shared-with-deps-2.6.0+cu124.zip
rm libtorch-cxx11-abi-shared-with-deps-2.6.0+cu124.zip
```

#### 4. Libtorch environment variables

In the `.bashrc` file, set the following libtorch environment variables. Here `<path_to_libtorch>` is the absolute path to the extracted `libtorch` folder from the previous step

```bash
export LIBTORCH=<path_to_libtorch>
export LIBTORCH_INCLUDE=<path_to_libtorch>
export LIBTORCH_LIB=<path_to_libtorch>
export LD_LIBRARY_PATH=<path_to_libtorch>/lib:$LD_LIBRARY_PATH
export CUDA_ROOT=/usr/local/cuda-12.4
```

This can also be achieved by making a `.cargo/config.toml` file in the checkout path

```toml
[env]
LIBTORCH=<path_to_libtorch>
LD_LIBRARY_PATH=<path_to_libtorch>/lib
CUDA_ROOT = "/usr/local/cuda-12.4"
```

#### 5. Download & install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### 6. (optional) Install `just`

```bash
sudo snap install just --edge --classic
```

#### 7. (optional) Install Solana and Anchor

Install Solana

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/beta/install)"
```

After installation, follow the instructions to add the Solana tools to PATH.

Install Anchor

```bash
cargo install --git https://github.com/coral-xyz/anchor --rev a7a23eea308440a9fa9cb79cee7bddd30ab163d5 anchor-cli
```

This may require

```bash
sudo apt install pkg-config libudev-dev libssl-dev
```

### Windows

1. Install CUDA libraries: https://developer.nvidia.com/cuda-12-4-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11

2. Download libtorch & extract: https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.6.0%2Bcu124.zip

3. Download OpenSSL: https://slproweb.com/download/Win64OpenSSL-3_3_2.exe

4. Install Perl: https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit.msi

5. Create a `.cargo/config.toml` file to set environment variables

**NOTE**: Building may take several minutes the first time as `openssl-sys` takes a long time (for some reason)

```
[env]
LIBTORCH = <path_to_libtorch>
OPENSSL_LIB_DIR = <path_to_openssl>/lib/VC/x64/MT
OPENSSL_INCLUDE_DIR = <path_to_openssl>/include
```

### MacOS / aarch64

These platforms aren't supported right now :(
PRs welcome!

### Docker

Create a Docker image with the necessary dependencies to run a Psyche client:

1. Install the necessary NVIDIA and CUDA drivers as explained in the previous sections.
2. Install the NVIDIA [container toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html). If using Ubuntu, just run:

```bash
sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit
```

3. Create an `.env` file following the `.env.example` in `psyche/config/client` and update the necessary environment variables.
4. Run `docker compose build`.

## Useful commands

Psyche uses [`just`](https://github.com/casey/just) to run some common tasks.

You can run `just` to see the whole list of commands!

### Running checks

> requires Nix!

```bash
just check
```

If it passes, CI will pass.

### Formatting

```bash
just fmt
```
