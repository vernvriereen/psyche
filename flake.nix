{
  description = "Rust shells";

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
        "x86_64-darwin"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem = {
        config,
        pkgs,
        system,
        ...
      }: let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];

          config.allowUnfree = true;
          config.cudaSupport = true;
          config.cudaVersion = "12.4";
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src"];
          targets = ["x86_64-unknown-linux-musl"];
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

        # build env only
        nativeBuildInputs = with pkgs; [
          pkg-config

          alejandra # nix fmtter
        ];

        # runtime env
        buildInputs = [torch pkgs.openssl];

        env = {
          LIBTORCH = torch.out;
          LIBTORCH_INCLUDE = torch.dev;
          LIBTORCH_LIB = torch.out;
        };

        commonArgs = {
          inherit env src buildInputs nativeBuildInputs;
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
        };
      };
    };
}
