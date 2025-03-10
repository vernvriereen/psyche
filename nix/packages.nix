{ inputs, ... }:
{
  perSystem =
    { system, ... }:
    let
      pkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [
          (import inputs.rust-overlay)
          inputs.nix-gl-host.overlays.default
        ];
        config.allowUnfree = true;
        config.cudaSupport = true;
        config.cudaVersion = "12.4";
      };

      inherit (import ./common.nix { inherit inputs system pkgs; })
        craneLib
        commonArgs
        buildPackage
        useHostGpuDrivers
        ;

      rustPackageNames = [
        "psyche-centralized-client"
        "psyche-centralized-server"
        "psyche-centralized-local-testnet"
        "expand-distro"
      ];
      rustPackages = builtins.listToAttrs (
        map (name: {
          inherit name;
          value = buildPackage name;
        }) rustPackageNames
      );
      nixglhostRustPackages = builtins.listToAttrs (
        map (name: {
          name = "${name}-nixglhost";
          value = useHostGpuDrivers rustPackages.${name};
        }) rustPackageNames
      );

      cudaDockerImage = pkgs.dockerTools.pullImage {
        imageName = "nvidia/cuda";
        imageDigest = "sha256:0f6bfcbf267e65123bcc2287e2153dedfc0f24772fb5ce84afe16ac4b2fada95";
        sha256 = "sha256-WFT1zPTFV9lAoNnT2jmKDB+rrbUSY9escmlO95pg4sA=";
      };
    in
    {
      packages =
        rustPackages
        // nixglhostRustPackages
        // rec {
          stream-docker-psyche-centralized-client = pkgs.dockerTools.streamLayeredImage {
            name = "docker.io/nousresearch/psyche-centralized-client";
            tag = "latest";
            fromImage = cudaDockerImage;
            config = {
              contents = [ pkgs.bash ];
              Env = [
                "RUST_BACKTRACE=1"
                "TUI=false"
              ];
              Entrypoint = [
                "${rustPackages.psyche-centralized-client}/bin/psyche-centralized-client"
                "train"
                "--checkpoint-dir"
                "./checkpoints"
              ];
            };
          };

          psyche-book = pkgs.stdenv.mkDerivation {
            name = "psyche-book";
            src = ../psyche-book;
            nativeBuildInputs = with pkgs; [
              mdbook
              mdbook-mermaid
              mdbook-linkcheck
            ];
            postPatch = ''
              mkdir -p generated/cli
              ${builtins.concatStringsSep "\n" (
                map (
                  name:
                  "${rustPackages.${name}}/bin/${name} print-all-help --markdown > generated/cli/${
                    builtins.replaceStrings [ "-" ] [ "-" ] name
                  }.md"
                ) rustPackageNames
              )}
            '';
            buildPhase = "mdbook build";
            installPhase = ''
              mkdir -p $out
              cp -r book/html/* $out/
            '';
          };
        };
    };
}
