{
  self,
  inputs,
  ...
}:
{
  perSystem =
    { system, pkgs, ... }:
    let
      inherit (pkgs.psycheLib)
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

          # for some build scripts
          jq
          # it pretty :3
          nix-output-monitor

          # for running pkgs on non-nix
          pkgs.nix-gl-host

          # solana
          inputs.solana-pkgs.packages.${system}.default

          # nixfmt
          nixfmt-rfc-style

          # for pnpm stuff
          nodejs
          pnpm
          wasm-pack
        ];
      };
    };
}
