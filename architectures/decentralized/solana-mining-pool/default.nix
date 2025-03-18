{
  pkgs,
  ...
}:
pkgs.psycheLib.buildSolanaIdl {
  src = pkgs.psycheLib.src;
  workspaceDir = ./.;
  sourceRoot = "source/architectures/decentralized/solana-mining-pool";
  programName = "solana-mining-pool";
  keypair = ../local-dev-keypair.json;
}
