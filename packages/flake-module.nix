# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage;
      artPackages = callPackage ./art { inherit pkgs; };
      pythonPackages = callPackage ./python { inherit pkgs; };
      goPackages = callPackage ./go { inherit pkgs; };
      rustPackages = callPackage ./rust { inherit pkgs; inherit (inputs) crane; };
    in
    {
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
    }
    // artPackages
    // pythonPackages
    // goPackages
    // rustPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: ghafpkgs;
}
