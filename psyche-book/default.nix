{
  pkgs,
  rustPackages,
  rustPackageNames,
  ...
}:
let
  mdbook-0-4-47 = pkgs.mdbook.overrideAttrs (
    oldAttrs:
    let
      version = "0.4.47";
      src = pkgs.fetchFromGitHub {
        owner = "rust-lang";
        repo = "mdBook";
        tag = "v${version}";
        hash = "sha256-XTvC2pGRVat0kOybNb9TziG32wDVexnFx2ahmpUFmaA=";
      };
    in
    {
      inherit version src;
      cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
        inherit (oldAttrs) pname;
        inherit version src;
        allowGitDependencies = false;
        hash = "sha256-ASPRBAB+elJuyXpPQBm3WI97wD3mjoO1hw0fNHc+KAw=";
      };
    }
  );
in
pkgs.stdenv.mkDerivation {
  name = "psyche-book";
  src = ./.;
  nativeBuildInputs = with pkgs; [
    mdbook-0-4-47
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
    cp ${../secrets.nix} generated/secrets.nix
  '';
  buildPhase = "mdbook build";
  installPhase = ''
    mkdir -p $out
    cp -r book/html/* $out/
  '';
}
