flake:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib)
    mkEnableOption
    mkOption
    types
    mkIf
    literalExpression
    ;

  cfg = config.services.gh-notify-bridge;
in
{
  options.services.gh-notify-bridge = {
    enable = mkEnableOption "gh-notify-bridge GitHub notification forwarder";

    package = mkOption {
      type = types.package;
      default = flake.packages.${pkgs.system}.gh-notify-bridge;
      defaultText = literalExpression "flake.packages.\${pkgs.system}.gh-notify-bridge";
      description = "The gh-notify-bridge package to use.";
    };

    githubTokenFile = mkOption {
      type = types.path;
      description = "Path to file containing GitHub PAT with `notifications` scope.";
      example = "/run/secrets/github-token";
    };

    port = mkOption {
      type = types.port;
      default = 8080;
      description = "HTTP server port. App registers its UP endpoint via POST /register.";
    };

    stateDir = mkOption {
      type = types.str;
      default = "/var/lib/gh-notify-bridge";
      description = "Directory to store state (registered endpoint, last poll timestamp).";
    };

    user = mkOption {
      type = types.str;
      default = "gh-notify-bridge";
      description = "User to run gh-notify-bridge as.";
    };

    group = mkOption {
      type = types.str;
      default = "gh-notify-bridge";
      description = "Group to run gh-notify-bridge as.";
    };
  };

  config = mkIf cfg.enable {
    users.users.${cfg.user} = {
      inherit (cfg) group;
      isSystemUser = true;
      description = "gh-notify-bridge service user";
      home = cfg.stateDir;
    };

    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.gh-notify-bridge = {
      description = "GitHub notification bridge to UnifiedPush";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.stateDir;
        Restart = "on-failure";
        RestartSec = "10s";

        # Hardening
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        RestrictNamespaces = true;
        ReadWritePaths = [ cfg.stateDir ];
      };

      environment = {
        PORT = toString cfg.port;
      };

      # Load token from file at runtime
      script = ''
        export GITHUB_TOKEN="$(cat ${cfg.githubTokenFile})"
        exec ${cfg.package}/bin/gh-notify-bridge
      '';
    };
  };
}
