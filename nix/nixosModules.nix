{
  self,
  inputs,
  ...
}: {
  flake = {
    # this is the server for hosting the docs with HTTP basic auth!
    # once we have made the repo public, we'll also have a non-authed version.
    nixosConfigurations."psyche-book-http-authed" = inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        inputs.agenix.nixosModules.default

        {
          age = {
            secrets.caddyBasicAuth = {
              file = ./secrets/docs-http-basic.age;
              owner = "caddy";
              group = "caddy";
            };
            identityPaths = [
              "/var/garnix/keys/repo-key"
            ];
          };
        }

        ({
          config,
          pkgs,
          ...
        }: let
          inherit (self.packages.${pkgs.system}) psyche-book;
        in {
          system.stateVersion = "24.11";
          fileSystems."/" = {
            device = "/dev/sda1";
            fsType = "ext4";
          };
          boot.loader.grub.device = "/dev/sda";
          services.caddy = {
            enable = true;
            virtualHosts."http://psyche-book-http-authed.*.psyche.NousResearch.garnix.me".extraConfig = ''
              root * ${psyche-book}
              file_server
              basic_auth {
                import ${config.age.secrets.caddyBasicAuth.path}
              }
            '';
          };
          networking.firewall.allowedTCPPorts = [80];
        })
      ];
    };
  };
}
