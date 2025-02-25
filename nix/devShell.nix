{
  self,
  inputs,
  ...
}:
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
        buildWholeWorkspace
        env
        ;
    in
    {
      devShells.default = pkgs.mkShell {
        inputsFrom = [
          buildWholeWorkspace
          self.packages.${system}.psyche-book
        ];
        inherit env;
        buildInputs = with pkgs; [
          # for local-testnet
          tmux
          nvtopPackages.full

          # task runner
          just

          # for running pkgs on non-nix
          pkgs.nix-gl-host

          # solana
          inputs.solana-pkgs.packages.${system}.default

          # nixfmt
          nixfmt-rfc-style
        ];
      };
    };
}
