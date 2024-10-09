{
  description = "Nous Psyche";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs @ {
    flake-parts,
    crane,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = {system, ...}: let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];

          config.allowUnfree = true;
          config.cudaSupport = true;
          config.cudaVersion = "12.4";
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src"];
        };
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        src = craneLib.cleanCargoSource ./.;

        torch = pkgs.libtorch-bin.dev.overrideAttrs (old: {
          version = "2.4.0";
          src = pkgs.fetchzip {
            name = "libtorch-cxx11-abi-shared-with-deps-2.4.0-cu124.zip";
            url = "https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.0%2Bcu124.zip";
            hash = "sha256-aned9QWMX5fe2U9igs4e2UjczmtwDq+v8z/feYnP9aQ=";
          };
        });

        env = {
          CUDA_ROOT = pkgs.cudaPackages.cudatoolkit.out;
          LIBTORCH = torch.out;
          LIBTORCH_INCLUDE = torch.dev;
          LIBTORCH_LIB = torch.out;
        };

        commonArgs = {
          inherit env src;
          strictDeps = true;

          # build env only
          nativeBuildInputs = with pkgs; [
            pkg-config

            alejandra # nix fmtter
          ];

          # runtime env
          buildInputs = [torch] ++ (with pkgs; [openssl]) ++ (with pkgs.cudaPackages; [cudatoolkit cuda_cudart nccl]);
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        bin = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
          });
      in {
        packages = {
          inherit bin;
          default = bin;
        };
        devShells.default = pkgs.mkShell {
          inputsFrom = [bin];
          inherit env;
          buildInputs = with pkgs; [
            tmux
            nvtopPackages.full
          ];
        };
      };
    };
}
