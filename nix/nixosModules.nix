{
  self,
  inputs,
  ...
}:
{
  flake =
    let
      keys = import ./keys.nix;
      backend-port = "3000";
      base-system = [
        inputs.garnix-lib.nixosModules.garnix
        inputs.agenix.nixosModules.default

        (
          { ... }:
          {
            # required minimal setup for garnix
            garnix.server.enable = true;

            # allow decrypting secrets
            age.identityPaths = [
              "/var/garnix/keys/repo-key"
            ];

            # assume we're doing webserver work
            networking.firewall.allowedTCPPorts = [
              80
              443
            ];

            # custom pkgs overlays
            nixpkgs = import ./pkgs.nix {
              inherit inputs;
              gitcommit = self.rev or self.dirtyRev;
            };

            system.stateVersion = "25.05";
          }
        )
      ];

      # lets devs SSH into these boxes. probably don't enable in production for safety.
      debug-ssh =
        { ... }:
        {
          services.openssh = {
            enable = true;
            settings = {
              PasswordAuthentication = false;
              KbdInteractiveAuthentication = false;
            };
          };
          users.users."nous" = {
            isNormalUser = true;
            home = "/home/nous";
            description = "debug ssh user";
            extraGroups = [ "wheel" ];
            openssh.authorizedKeys.keys = keys.allKeys;
          };
          security.sudo.wheelNeedsPassword = false;
        };

      psyche-website-backend =
        secret-file:
        {
          config,
          pkgs,
          ...
        }:
        let
          psyche-website-backend = pkgs.callPackage ../website/backend { };
        in
        {
          age.secrets.backendRpc = {
            file = secret-file;
            owner = "root";
            group = "root";
          };

          systemd.services.backend = {
            description = "psyche website backend";
            wantedBy = [ "multi-user.target" ];
            wants = [ "network-online.target" ];
            after = [ "network-online.target" ];
            environment = {
              PORT = backend-port;
              # the RPC urls, alongside the contract addrs, are defined in the EnvironmentFile since they contain secrets.
            };
            serviceConfig = {
              Type = "simple";
              StateDirectory = "backend";
              DynamicUser = true;
              EnvironmentFile = config.age.secrets.backendRpc.path;
              # don't start until we have DNS!
              ExecStartPre = "/bin/sh -c 'until ${pkgs.bind.host}/bin/host example.com; do sleep 1; done'";
              ExecStart = pkgs.lib.getExe psyche-website-backend;
            };
          };
        };

      persistentPsycheWebsite =
        {
          configName,
          backendSecret,
          miningPoolRpc,
          hostnames ? [ ],
        }:
        inputs.nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = base-system ++ [
            debug-ssh
            (psyche-website-backend backendSecret)
            (
              {
                config,
                pkgs,
                ...
              }:
              let
                backendPath = "/api";
                psyche-website-frontend = pkgs.callPackage ../website/frontend {
                  inherit miningPoolRpc backendPath;
                };
              in
              {
                garnix.server.persistence = {
                  enable = true;
                  name = configName;
                };
                age.secrets.caddyBasicAuth = {
                  file = ../secrets/docs-http-basic.age;
                  owner = "caddy";
                  group = "caddy";
                };
                services.caddy =
                  let
                    cfg = ''
                      handle {
                        root * ${psyche-website-frontend}
                        try_files {path} /index.html
                        file_server
                      }

                      handle_path ${backendPath}/* {
                        reverse_proxy :${backend-port}
                      }

                      basic_auth {
                        import ${config.age.secrets.caddyBasicAuth.path}
                      }
                    '';
                  in
                  {
                    enable = true;
                    virtualHosts =
                      {
                        "http://${configName}.*.psyche.NousResearch.garnix.me".extraConfig = cfg;
                      }
                      // (builtins.listToAttrs (
                        map (hostname: {
                          name = hostname;
                          value = {
                            extraConfig = cfg;
                          };
                        }) hostnames
                      ));
                  };
              }
            )
          ];
        };
    in
    {
      # server for hosting the frontend/backend with http basic auth, for testing
      nixosConfigurations."psyche-http-devnet-authed" = persistentPsycheWebsite {
        configName = "psyche-http-devnet-authed";
        hostnames = [ "devnet-preview.psyche.network" ];
        backendSecret = ../secrets/devnet/backend.age;
        miningPoolRpc = "https://api.devnet.solana.com";
      };
      nixosConfigurations."psyche-http-mainnet-authed" = persistentPsycheWebsite {
        configName = "psyche-http-mainnet-authed";
        hostnames = [ "mainnet-preview.psyche.network" ];
        backendSecret = ../secrets/backend-mainnet.age;
        miningPoolRpc = "https://api.mainnet-beta.solana.com/";
      };

      # server for hosting with no auth, for main
      nixosConfigurations."psyche-http-mainnet" = inputs.nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = base-system ++ [
          debug-ssh
          (psyche-website-backend ../secrets/backend-mainnet.age)
          (
            {
              config,
              pkgs,
              ...
            }:
            let
              backendPath = "/api";
              psyche-website-frontend = pkgs.callPackage ../website/frontend {
                miningPoolRpc = "https://api.mainnet-beta.solana.com/";
                inherit backendPath;
              };
            in
            {
              # don't lose our nice db every time we deploy!
              garnix.server.persistence = {
                enable = true;
                name = "psyche-mainnet";
              };

              services.caddy = {
                enable = true;
                virtualHosts = {
                  "https://docs.psyche.network".extraConfig = ''
                    root * ${self.packages.${pkgs.system}.psyche-book}
                    file_server
                  '';

                  "https://mainnet.psyche.network".extraConfig = ''
                    handle {
                      root * ${psyche-website-frontend}
                      try_files {path} /index.html
                      file_server
                    }

                    handle_path ${backendPath}/* {
                      reverse_proxy :${backend-port}
                    }
                  '';
                };
              };
            }
          )
        ];
      };

      # server for hosting docs with auth, for test deploys
      nixosConfigurations."psyche-http-docs-authed" = inputs.nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = base-system ++ [
          (
            { pkgs, ... }:
            {
              services.caddy = {
                enable = true;
                virtualHosts."http://psyche-http-docs-authed.test-deploy-docs.psyche.NousResearch.garnix.me".extraConfig =
                  ''
                    root * ${self.packages.${pkgs.system}.psyche-book}
                    file_server
                  '';
              };
            }
          )
        ];
      };
    };
}
