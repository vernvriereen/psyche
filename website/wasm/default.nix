{ pkgs, system, ... }:
let
  wasm = pkgs.psycheLib.buildRustWasmTsPackage "psyche-deserialize-zerocopy-wasm";
in
pkgs.stdenv.mkDerivation {
  name = "psyche-website-wasm";

  src = [
    ./fixup.sh
  ];

  unpackPhase = ''
    for srcFile in $src; do
      cp $srcFile $(stripHash $srcFile)
    done
  '';

  buildPhase = ''
    runHook preBuild

    echo "copying pkg..."
    cp -r ${wasm}/pkg .
    chmod 775 pkg -R

    echo "copying bindings..."
    cp -r ${wasm}/bindings .
    chmod 775 bindings -R 

    echo "running fixup..."
    ${pkgs.bash}/bin/bash ./fixup.sh

    mkdir $out
    cp -r pkg $out

    runHook postBuild
  '';
}
