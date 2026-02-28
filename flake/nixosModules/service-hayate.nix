{
  flake.nixosModules.service-hayate = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.services.hayate;
  in
    with lib; {
      options.services.hayate = {
        enable = mkEnableOption "hayate UTxORPC indexer";

        package = mkOption {
          type = types.package;
          default = pkgs.hayate;
          description = "The hayate package to use";
        };

        network = mkOption {
          type = types.enum ["mainnet" "preprod" "preview" "sanchonet"];
          default = "mainnet";
          description = "Cardano network to index";
        };

        socketPath = mkOption {
          type = types.path;
          description = "Path to cardano-node socket";
        };

        hostAddr = mkOption {
          type = types.str;
          default = "127.0.0.1";
          description = "Address to bind UTxORPC server";
        };

        port = mkOption {
          type = types.port;
          default = 50051;
          description = "Port for UTxORPC server";
        };

        # Token/wallet tracking
        tokens = mkOption {
          type = types.listOf (types.submodule {
            options.policy_id = mkOption {
              type = types.str;
              description = "Policy ID of token to track";
            };
          });
          default = [];
          description = "List of tokens to track by policy ID";
        };

        wallets = mkOption {
          type = types.listOf types.str;
          default = [];
          description = "List of wallet extended public keys to track";
        };

        addresses = mkOption {
          type = types.listOf types.str;
          default = [];
          description = "List of addresses to track";
        };

        user = mkOption {
          type = types.str;
          default = "hayate";
          description = "User to run hayate service as";
        };

        group = mkOption {
          type = types.str;
          default = "hayate";
          description = "Group for hayate service";
        };
      };

      config = mkIf cfg.enable {
        users.users.${cfg.user} = {
          isSystemUser = true;
          group = cfg.group;
        };

        users.groups.${cfg.group} = {};

        # Generate hayate config
        environment.etc."hayate/config.toml".text = ''
          ${optionalString (cfg.tokens != []) ''
            tokens = [
              ${concatMapStringsSep "\n  " (t: ''{ policy_id = "${t.policy_id}" }'') cfg.tokens}
            ]
          ''}

          ${optionalString (cfg.wallets != []) ''
            wallets = [
              ${concatMapStringsSep "\n  " (w: ''"${w}"'') cfg.wallets}
            ]
          ''}

          ${optionalString (cfg.addresses != []) ''
            addresses = [
              ${concatMapStringsSep "\n  " (a: ''"${a}"'') cfg.addresses}
            ]
          ''}
        '';

        systemd.services.hayate = {
          description = "Hayate UTxORPC Indexer";
          wantedBy = ["multi-user.target"];
          after = ["cardano-node.service"];
          requires = ["cardano-node.service"];

          environment = {
            RUST_LOG = "hayate::api=debug,hayate=info,h2=warn";
          };

          # Wait for socket before starting
          preStart = ''
            while [ ! -S "${cfg.socketPath}" ]; do
              echo "Waiting for cardano-node socket at ${cfg.socketPath}..."
              sleep 5
            done
            echo "Socket found, starting hayate..."
          '';

          serviceConfig = {
            Type = "simple";
            User = cfg.user;
            Group = cfg.group;
            SupplementaryGroups = ["cardano-node"];
            Restart = "always";
            RestartSec = "30s";
            TimeoutStartSec = "600";

            StateDirectory = "hayate";

            ExecStart = ''
              ${cfg.package}/bin/hayate sync \
                --config /etc/hayate/config.toml \
                --network ${cfg.network} \
                --socket ${cfg.socketPath} \
                --db-path /var/lib/hayate
            '';

            # Resource limits
            MemoryMax = "4G";

            # Hardening
            NoNewPrivileges = true;
            PrivateTmp = true;
            ProtectSystem = "full";
            ProtectHome = true;
            ReadWritePaths = ["/var/lib/hayate"];
          };
        };

        # Firewall
        networking.firewall.allowedTCPPorts = mkIf (cfg.hostAddr != "127.0.0.1") [cfg.port];
      };
    };
}
