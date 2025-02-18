# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, lib, ... }:
{
  imports = [
    inputs.devshell.flakeModule
  ];
  perSystem =
    { config, pkgs, ... }:
    {
      devshells.default.devshell = {
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
          config.treefmt.build.wrapper
        ] ++ lib.attrValues config.treefmt.build.programs; # make all the trefmt packages available
      };
    };
}
