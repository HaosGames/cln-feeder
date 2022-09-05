{ config, pkgs, lib, ... }:

let
  cfg = config.services.cln-feeder;
  inherit (lib) mkOption mkEnableOption types mkIf;
in
{
  options = {
    services.cln-feeder = {
      enable = mkEnableOption "TEOS Watchtower Daemon";
      user = mkOption {
        type = types.str;
        description = "The user as which to run cln-feeder. It has to have permissions to access the CLN Socket";
      };
      group = mkOption {
        type = types.str;
        description = "The group as which to run cln-feeder";
      };
      socket = mkOption {
        type = types.path;
        description = "Path to the CLN Socket";
      };
      dataDir = mkOption {
        type = types.path;
        default = "/var/lib/cln-feeder";
        description = "The data directory for cln-feeder";
      };
      package = mkOption {
        type = types.package;
        default = pkgs.cln-feeder;
        description = "The package providing cln-feeder binaries";
      };
      feeAdjustment = mkOption {
          type = types.ints.positive;
          default = 20;
          description = "Fee adjustment";
      };
      epochs = mkOption {
        type = types.ints.positive;
        default = 1;
        description = "Past epochs to take into account when calculating new fees";
      };
      epochLength = mkOption {
        type = types.ints.positive;
        default = 12;
        description = "The length of an epoch in hours";
      };
      extraArgs = mkOption {
        type = types.str;
        default = "";
        description = "Extra cli arguments appended to the command executing the binary";
      };
    };
  };
  config =
  let
    executionCommand = "${cfg.package}/bin/cln-feeder --data-dir=${cfg.dataDir} --socket=${cfg.socket} --epochs=${cfg.epochs} --epoch-length=${cfg.epochLength} --fee-adjustment=${cfg.feeAdjustment} ${cfg.extraArgs}";
  in
  mkIf cfg.enable {
    systemd.services.cln-feeder = {
      enable = true;
      description = "cln-feeder";
      wantedBy = [ "multi-user.target" ];
      after = [ "clightning.service" ];
      requires = [ "clightning.service" ];
      serviceConfig = {
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.package.src;
        StateDirectory = cfg.dataDir;
        ExecStart = executionCommand;
        Restart = "always";
        RestartSec = "10s";
      };
    };
    environment.systemPackages = [ cfg.package ];
  };
}

