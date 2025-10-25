# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
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
            pkgs.cmake-language-server

            # Development utilities (includes all package managers as dependencies)
            self'.packages.update-deps

            config.treefmt.build.wrapper
          ]
          ++ config.pre-commit.settings.enabledPackages
          ++ lib.attrValues config.treefmt.build.programs; # make all the trefmt packages available

          startup.hook.text = config.pre-commit.installationScript;

          packagesFrom =
            let
              # Filter out function attributes like 'override' and 'overrideDerivation'
              isPackage =
                name: _value:
                !(lib.elem name [
                  "override"
                  "overrideDerivation"
                ]);
              packageAttrs = lib.filterAttrs isPackage self'.packages;
            in
            builtins.attrValues packageAttrs ++ self'.packages.ghaf-audio-control.buildInputs;
        };
        # TODO: what is using the below?
        env = [
          {
            name = "PKG_CONFIG_PATH";
            prefix = "$DEVSHELL_DIR/lib/pkgconfig";
          }
        ];
      };
    };
}
