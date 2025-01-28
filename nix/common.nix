{
  inputs,
  system,
  pkgs,
}: let
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = ["rust-src"];
  };
  craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  solanaRustToolchain = inputs.solana-pkgs.packages.${system}.solana-rust;

  testResourcesFilter = path: _type:
    (builtins.match ".*tests/resources/.*$" path != null)
    || (builtins.match ".*.config/.*$" path != null)
    || (builtins.match ".*local_dev*-keypair.json$" != null);

  src = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: (testResourcesFilter path type) || (craneLib.filterCargoSources path type);
  };

  torch = pkgs.libtorch-bin.dev.overrideAttrs (
    old:
    let
      version = "2.6.0";
      cuda = "124";
    in
    {
      version = version;
      src = pkgs.fetchzip {
        name = "libtorch-cxx11-abi-shared-with-deps-${version}-cu${cuda}.zip";
        url = "https://download.pytorch.org/libtorch/cu${cuda}/libtorch-cxx11-abi-shared-with-deps-${version}%2Bcu${cuda}.zip";
        hash = "sha256-rJIvNGcR/xFfkr/O2a32CmO5/5Fv24Y7+efOtYgGO6A=";
      };
    }
  );

  env = {
    CUDA_ROOT = pkgs.cudaPackages.cudatoolkit.out;
    LIBTORCH = torch.out;
    LIBTORCH_INCLUDE = torch.dev;
    LIBTORCH_LIB = torch.out;
  };

  rustWorkspaceDeps = {
    nativeBuildInputs = with pkgs; [
      pkg-config
      perl
    ];

    buildInputs = [torch] ++ (with pkgs; [openssl]) ++ (with pkgs.cudaPackages; [cudatoolkit cuda_cudart nccl]);
  };

  rustWorkspaceArgs = rustWorkspaceDeps // {
    inherit env src;
    strictDeps = true;
  };

  cargoArtifacts = craneLib.buildDepsOnly rustWorkspaceArgs;

  buildPackage = name:
    craneLib.buildPackage (rustWorkspaceArgs
      // {
        inherit cargoArtifacts;
        pname = name;
        cargoExtraArgs = "--bin ${name}";
        doCheck = false;
      });

  buildWholeWorkspace = craneLib.buildPackage (rustWorkspaceArgs
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
            --run 'exec ${pkgs.nix-gl-host}/bin/nixglhost "'"$bin"'" -- "$@"'
        fi
      done
    '';

  solanaCraneLib = (inputs.crane.mkLib pkgs).overrideToolchain inputs.solana-pkgs.packages.${system}.solana-rust;

  # output the package's idl.json
  buildSolanaIdl = {
    src,
    programName,
    workspaceDir,
    sourceRoot,
    keypair ? "",
  } @ origArgs: let
    args = builtins.removeAttrs origArgs [
      "keypair"
      "programName"
    ];
    cargoLock = workspaceDir + "/Cargo.lock";
    cargoToml = workspaceDir + "/Cargo.lock";

    env = {
        RUSTFLAGS = "--cfg procmacro2_semver_exempt -A warnings";
      };
      solanaWorkspaceArgs =  rustWorkspaceDeps // {
      inherit env src sourceRoot cargoLock;
    };
    solanaCargoArtifacts = solanaCraneLib.buildDepsOnly (solanaWorkspaceArgs // {
      buildPhaseCargoCommand = "cargo test --no-run";
    });
  in
    solanaCraneLib.mkCargoDerivation (solanaWorkspaceArgs // {
        cargoArtifacts = solanaCargoArtifacts;
        pname = programName;
        version = "0";
        pnameSuffix = "-idl";

        ANCHOR_IDL_BUILD_PROGRAM_PATH = "./programs/${programName}";

        postPatch = ''
          if [ -n "${keypair}" ]; then
            mkdir -p ./target/deploy
            cp ${keypair} ./target/deploy/psyche_solana_coordinator-keypair.json
          fi
        '';

        nativeBuildInputs = [inputs.solana-pkgs.packages.${system}.anchor] ++ rustWorkspaceDeps.nativeBuildInputs;

        buildPhaseCargoCommand = ''
          mkdir $out
          anchor idl build --out $out/idl.json --out-ts $out/idlType.ts
        '';

        doInstallCargoArtifacts = false;
      });
in {
  inherit craneLib buildSolanaIdl rustWorkspaceArgs cargoArtifacts buildPackage buildWholeWorkspace useHostGpuDrivers env src;
}
