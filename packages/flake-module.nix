# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  # Build all ghafpkgs packages given a pkgs set
  # This function is reused by both perSystem.packages and the overlay
  mkGhafpkgs =
    { pkgs, crane }:
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

      artPackages = filterPackages (callPackage ./art { });
      pythonPackages = filterPackages (callPackage ./python { });
      goPackages = filterPackages (callPackage ./go { });
      rustPackages = filterPackages (callPackage ./rust { inherit crane; });
      cppPackages = filterPackages (callPackage ./cpp { });

      # Utility packages
      utilityPackages = {
        update-deps = callPackage ./update-deps { };
      };
    in
    artPackages // pythonPackages // goPackages // rustPackages // cppPackages // utilityPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = mkGhafpkgs {
        inherit pkgs;
        inherit (inputs) crane;
      };
    };

  # Overlay for use by downstream consumers
  # Provides all ghafpkgs packages when applied to a nixpkgs set
  flake.overlays.default =
    _final: prev:
    mkGhafpkgs {
      pkgs = prev;
      inherit (inputs) crane;
    };
}
