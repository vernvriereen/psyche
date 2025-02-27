{
  description = "Nous Psyche";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    agenix = {
      url = "github:ryantm/agenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
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
    garnix-lib.url = "github:garnix-io/garnix-lib";
    solana-pkgs.url = "github:arilotter/solana-flake";
  };

  outputs =
    inputs@{
      self,
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
      ];
      perSystem =
        { system, ... }:
        {
          _module.args.pkgs = (
            import inputs.nixpkgs (
              {
                inherit system;
              }
              // (import ./nix/pkgs.nix {
                inherit inputs;
                gitcommit = self.rev or self.dirtyRev;
              })
            )
          );
        };
      imports = [
        ./nix/packages.nix
        ./nix/devShell.nix
        ./nix/checks.nix
        ./nix/nixosModules.nix
      ];
    };
}
