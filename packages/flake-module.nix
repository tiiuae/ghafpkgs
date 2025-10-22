# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage lib;

      # Filter function to remove override attributes from package sets
      filterPackages =
        packageSet:
        lib.filterAttrs (
          name: _value:
          !(lib.elem name [
            "override"
            "overrideDerivation"
          ])
        ) packageSet;

      artPackages = filterPackages (callPackage ./art { inherit pkgs; });
      pythonPackages = filterPackages (callPackage ./python { inherit pkgs; });
      goPackages = filterPackages (callPackage ./go { inherit pkgs; });
      rustPackages = filterPackages (
        callPackage ./rust {
          inherit pkgs;
          inherit (inputs) crane;
        }
      );
      cppPackages = filterPackages (callPackage ./cpp { inherit pkgs; });
    in
    artPackages // pythonPackages // goPackages // rustPackages // cppPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: ghafpkgs;
}
