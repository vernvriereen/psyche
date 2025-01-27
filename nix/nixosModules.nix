{
  self,
  inputs,
  ...
}: {
  flake = let
    garnix-required = {
      system.stateVersion = "24.11";
      fileSystems."/" = {
        device = "/dev/sda1";
        fsType = "ext4";
      };
      boot.loader.grub.device = "/dev/sda";
    };
  in {
    # server for hosting the docs with http basic auth, for our branches
    nixosConfigurations."psyche-book-http-authed" = inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        inputs.agenix.nixosModules.default
        ({
          config,
          pkgs,
          ...
        }: let
          inherit (self.packages.${pkgs.system}) psyche-book;
        in
          garnix-required
          // {
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

    # server for hosting the docs with no auth, for main
    nixosConfigurations."psyche-book-http" = inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({
          config,
          pkgs,
          ...
        }: let
          inherit (self.packages.${pkgs.system}) psyche-book;
        in
          garnix-required
          // {
            services.caddy = let
              caddyConfig = {
                extraConfig = ''
                  root * ${psyche-book}
                  file_server
                '';
              };
            in {
              enable = true;
              virtualHosts = {
                "http://docs.psyche.network" = caddyConfig;
                "http://psyche-book-http.*.psyche.NousResearch.garnix.me" = caddyConfig;
                "http://psyche.network".extraConfig = ''
                  header Content-Type text/html
                  respond "<head><meta http-equiv='refresh' content='0; url=http://nousresearch.com/nous-psyche'></head>"
                '';
              };
            };
            networking.firewall.allowedTCPPorts = [80];
          })
      ];
    };
  };
}
