# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  # Build all ghafpkgs packages given a pkgs set
  # This function is reused by both perSystem.packages and the overlay
  #
  # Uses `import` instead of `callPackage` for category directories so that
  # the resulting attrset has statically-known attribute names.  This keeps the
  # overlay lazy: nixpkgs's fixed-point can determine which names the overlay
  # contributes without forcing `pkgs.callPackage` (which would cause infinite
  # recursion when the overlay is composed with other overlays via
  # `composeManyExtensions`).
  mkGhafpkgs =
    { pkgs, crane }:
    let
      inherit (pkgs) callPackage python3Packages;

      artPackages = import ./art { inherit callPackage; };
      pythonPackages = import ./python { inherit python3Packages; };
      goPackages = import ./go { inherit callPackage; };
      rustPackages = import ./rust {
        inherit callPackage;
        inherit crane;
      };
      cppPackages = import ./cpp { inherit callPackage; };

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
