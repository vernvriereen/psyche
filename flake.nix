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
    solana-pkgs.url = "github:arilotter/solana-flake";
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-parts,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      imports = [
        (import ./nix/packages.nix {inherit self inputs;})
        (import ./nix/devShell.nix {inherit self inputs;})
        (import ./nix/checks.nix {inherit self inputs;})
        (import ./nix/nixosModules.nix {inherit self inputs;})
      ];
    };
}
