{ pkgs, ... }:
let
  solana-coordinator-idl = pkgs.callPackage ../../architectures/decentralized/solana-coordinator { };
  solana-mining-pool-idl = pkgs.callPackage ../../architectures/decentralized/solana-mining-pool { };
  mkWebsitePackage = pkgs.callPackage ../common.nix { };
in
mkWebsitePackage {
  package = "shared";

  preBuild = ''
    mkdir -p shared/idl/
    pushd shared/idl/
      cp ${solana-coordinator-idl}/idl.json ./coordinator_idl.json
      cp ${solana-coordinator-idl}/idlType.ts ./coordinator_idlType.ts
      cp ${solana-mining-pool-idl}/idl.json ./mining-pool_idl.json
      cp ${solana-mining-pool-idl}/idlType.ts ./mining-pool_idlType.ts
    popd
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r shared $out/
    runHook postInstall
  '';
}
