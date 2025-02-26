{
  self,
  inputs,
  ...
}: {
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
      src
      craneLib
      commonArgs
      cargoArtifacts
      ;
  in {
    checks =
      self.packages.${system}
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
            RUST_LOG = "info,psyche=trace";
            partitions = 1;
            partitionType = "count";
            cargoNextestExtraArgs = "--workspace --exclude psyche-decentralized-testing";
          });

        validate-all-configs = pkgs.runCommand "validate-configs" {} ''
          dir="${../config}"
          if [ ! -d "$dir" ]; then
            echo "config dir $dir does not exist."
            exit 1
          fi

          for f in $dir/*; do
              if [ -f $f/data.toml ]; then
                ${self.packages.${system}.psyche-centralized-server}/bin/psyche-centralized-server validate-config --state $f/state.toml --data-config $f/data.toml || exit 1
                echo "config $f/data.toml and $f/state.toml ok!"
              else
                ${self.packages.${system}.psyche-centralized-server}/bin/psyche-centralized-server validate-config --state $f/state.toml|| exit 1
                echo "config $f/state.toml ok!"
              fi
          done;

          echo "all configs ok!"

          touch $out
        '';
      };
  };
}
