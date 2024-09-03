{
  description = "Rust shells";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs @ {flake-parts, ...}:
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
          config.cudaVersion = "12";
        };
      in {
        devShells.default = let
          torch = pkgs.libtorch-bin.dev.overrideAttrs (old: {
            version = "2.4.0";
            src = pkgs.fetchzip {
              name = "libtorch-cxx11-abi-shared-with-deps-2.4.0-cu121.zip";
              url = "https://download.pytorch.org/libtorch/cu121/libtorch-cxx11-abi-shared-with-deps-2.4.0%2Bcu121.zip";
              hash = "sha256-5HfCJ970GY3iqTKQD0GN0+FOVzCvBmZb0qXxcToTBHE=";
            };
          });
        in
          pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
            ];

            buildInputs = with pkgs;
              [
                torch
                (rust-bin.stable.latest.default.override {
                  extensions = ["rust-src"];
                  targets = ["x86_64-unknown-linux-musl"];
                })
                alejandra # nix fmtter

                # deps
                openssl
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                pkgs.linuxPackages_latest.perf
              ];

            LIBTORCH = torch;
          };
      };
    };
}
