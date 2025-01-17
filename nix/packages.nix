{inputs, ...}: {
  perSystem = {system, ...}: let
    pkgs = import inputs.nixpkgs {
      inherit system;
      overlays = [(import inputs.rust-overlay) inputs.nix-gl-host.overlays.default];
      config.allowUnfree = true;
      config.cudaSupport = true;
      config.cudaVersion = "12.4";
    };

    inherit
      (import ./common.nix {inherit inputs system pkgs;})
      craneLib
      commonArgs
      buildPackage
      useHostGpuDrivers
      ;
  in {
    packages = rec {
      psyche-centralized-client = useHostGpuDrivers (buildPackage "psyche-centralized-client");
      psyche-centralized-server = buildPackage "psyche-centralized-server";
      expand-distro = useHostGpuDrivers (buildPackage "expand-distro");

      stream-docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
        name = "docker.io/nousresearch/psyche-centralized-client";
        tag = "latest";
        fromImage = pkgs.dockerTools.pullImage {
          imageName = "nvidia/cuda";
          imageDigest = "sha256:0f6bfcbf267e65123bcc2287e2153dedfc0f24772fb5ce84afe16ac4b2fada95";
          sha256 = "sha256-WFT1zPTFV9lAoNnT2jmKDB+rrbUSY9escmlO95pg4sA=";
        };
        config = {
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

      psyche-book = pkgs.stdenv.mkDerivation {
        name = "psyche-book";
        src = ../psyche-book;
        nativeBuildInputs = with pkgs; [mdbook mdbook-mermaid];
        buildPhase = "mdbook build";
        installPhase = ''
          mkdir -p $out
          cp -r book/* $out/
        '';
      };
    };
  };
}
