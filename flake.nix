{
  description = "Nous Psyche";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    nix-gl-host = {
      url = "github:arilotter/nix-gl-host-rs";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        crane.follows = "crane";
        rust-overlay.follows = "rust-overlay";
      };
    };
  };

  outputs = inputs @ {
    flake-parts,
    crane,
    rust-overlay,
    nix-gl-host,
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
          overlays = [(import rust-overlay) nix-gl-host.overlays.default];

          config.allowUnfree = true;
          config.cudaSupport = true;
          config.cudaVersion = "12.4";
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src"];
        };
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        testResourcesFilter = path: _type:
          (builtins.match ".*tests/resources/.*$" path != null)
          || (builtins.match ".*.config/.*$" path != null);
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type: (testResourcesFilter path type) || (craneLib.filterCargoSources path type);
        };

        torch = pkgs.libtorch-bin.dev.overrideAttrs (old: let
          version = "2.4.1";
          cuda = "124";
        in {
          version = version;
          src = pkgs.fetchzip {
            name = "libtorch-cxx11-abi-shared-with-deps-${version}-cu${cuda}.zip";
            url = "https://download.pytorch.org/libtorch/cu${cuda}/libtorch-cxx11-abi-shared-with-deps-${version}%2Bcu${cuda}.zip";
            hash = "sha256-/MKmr4RnF2FSGjheJc4221K38TWweWAtAbCVYzGSPZM=";
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

          nativeBuildInputs = with pkgs; [
            alejandra # nix fmtter

            pkg-config # for Rust crates to find their native deps: CUDA, OpenSSL, etc.

            perl # needed to build OpenSSL Rust crate
          ];

          # dynamicly linked, used at runtime
          buildInputs = [torch] ++ (with pkgs; [openssl]) ++ (with pkgs.cudaPackages; [cudatoolkit cuda_cudart nccl]);
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        buildPackage = name:
          craneLib.buildPackage (commonArgs
            // {
              inherit cargoArtifacts;
              pname = name;
              cargoExtraArgs = "--bin ${name}";
              doCheck = false; # tests are run with nextest in `nix flake check`
            });

        buildWholeWorkspace = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
          });

        useHostGpuDrivers = package:
          pkgs.runCommand "${package.name}-nixgl-wrapped" {
            nativeBuildInputs = [pkgs.makeWrapper];
          } ''
            mkdir -p $out/bin
            for bin in ${package}/bin/*; do
              if [ -f "$bin" ] && [ -x "$bin" ]; then
                makeWrapper "$bin" "$out/bin/$(basename $bin)" \
                  --run 'exec ${pkgs.nix-gl-host}/bin/nixglhost "'"$bin"'" "$@"'
              fi
            done
          '';
      in rec {
        packages = rec {
          psyche-centralized-client = useHostGpuDrivers (buildPackage "psyche-centralized-client");
          psyche-centralized-server = buildPackage "psyche-centralized-server";
          expand-distro = useHostGpuDrivers (buildPackage "expand-distro");
          stream-docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
            name = "docker.io/nousresearch/psyche-centralized-client";
            tag = "latest";
            fromImage = pkgs.dockerTools.pullImage {
              imageName = "nvidia/cuda";
              imageDigest = "sha256:0f6bfcbf267e65123bcc2287e2153dedfc0f24772fb5ce84afe16ac4b2fada95"; # tag 12.4.1-base-ubuntu22.04
              sha256 = "sha256-WFT1zPTFV9lAoNnT2jmKDB+rrbUSY9escmlO95pg4sA=";
            };
            config = {
              # it's nice to have bash around for debugging.
              contents = [pkgs.bash];
              Env = [
                "RUST_BACKTRACE=1"
                "TUI=false"
              ];
              Entrypoint = [
                "${psyche-centralized-client}/bin/psyche-centralized-client"
                "train"
                "--checkpoint-dir"
                "./checkpoints"
              ];
            };
          };
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [buildWholeWorkspace];
          inherit env;
          buildInputs = with pkgs; [
            # for the ./local.sh script
            tmux
            nvtopPackages.full

            # task runner~
            just

            # running things on non-NixOS
            pkgs.nix-gl-host
          ];
        };

        checks =
          packages
          // {
            workspace-format = craneLib.cargoFmt {
              inherit src;
            };

            workspace-clippy = craneLib.cargoClippy (commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--workspace -- --deny warnings";
              });

            workspace-test = craneLib.cargoNextest (commonArgs
              // {
                inherit cargoArtifacts;
                partitions = 1;
                partitionType = "count";
              });

            validate-all-configs = pkgs.runCommand "validate-configs" {} ''
              dir="${./config}"
              if [ ! -d "$dir" ]; then
                echo "config dir $dir does not exist."
                exit 1
              fi

              for f in $dir/*; do
                  if [ -f $f/data.toml ]; then
                    ${packages.psyche-centralized-server}/bin/psyche-centralized-server --state $f/state.toml --data-config $f/data.toml validate-config || exit 1
                    echo "config $f/data.toml and $f/state.toml ok!"
                  else
                    ${packages.psyche-centralized-server}/bin/psyche-centralized-server --state $f/state.toml validate-config || exit 1
                    echo "config $f/state.toml ok!"
                  fi
              done;

              echo "all configs ok!"

              touch $out
            '';
          };
      };
    };
}
