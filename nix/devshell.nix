# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, lib, ... }:
{
  imports = [
    inputs.devshell.flakeModule
  ];
  perSystem =
    {
      self',
      pkgs,
      config,
      ...
    }:
    {
      devshells.default = {
        devshell = {
          name = "Ghafpkgs devshell";
          meta.description = "Ghafpkgs development environment";
          packages = [
            pkgs.bashInteractive
            pkgs.imagemagick
            pkgs.nixVersions.latest
            pkgs.nix-eval-jobs
            pkgs.nix-fast-build
            pkgs.nix-output-monitor
            pkgs.nix-tree
            pkgs.reuse

            pkgs.stdenv.cc
            pkgs.clippy

            config.treefmt.build.wrapper
          ] ++ lib.attrValues config.treefmt.build.programs; # make all the trefmt packages available

          packagesFrom = builtins.attrValues self'.packages ++ self'.packages.ghaf-audio-control.buildInputs;
        };
        env = [
          {
            name = "PKG_CONFIG_PATH";
            prefix = "$DEVSHELL_DIR/lib/pkgconfig";
          }
        ];
      };
    };
}
