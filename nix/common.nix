{
  inputs,
  system,
  pkgs,
}: let
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = ["rust-src"];
  };
  craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  testResourcesFilter = path: _type:
    (builtins.match ".*tests/resources/.*$" path != null)
    || (builtins.match ".*.config/.*$" path != null);
  src = pkgs.lib.cleanSourceWith {
    src = ../.;
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
      alejandra
      pkg-config
      perl
    ];

    buildInputs = [torch] ++ (with pkgs; [openssl]) ++ (with pkgs.cudaPackages; [cudatoolkit cuda_cudart nccl]);
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  buildPackage = name:
    craneLib.buildPackage (commonArgs
      // {
        inherit cargoArtifacts;
        pname = name;
        cargoExtraArgs = "--bin ${name}";
        doCheck = false;
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
            --run 'exec ${pkgs.nix-gl-host}/bin/nixglhost "'"$bin"'" -- "$@"'
        fi
      done
    '';
in {
  inherit craneLib commonArgs cargoArtifacts buildPackage buildWholeWorkspace useHostGpuDrivers env src;
}
