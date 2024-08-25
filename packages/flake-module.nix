# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
_:
let
  ghafpkgs =
    pkgs:
    let
      callPackage = pkgs.lib.callPackageWith pkgs;
    in
    {

      ghaf-artwork = callPackage ./ghaf-artwork { };
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
      ghaf-theme = callPackage ./ghaf-theme { };
    };
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: ghafpkgs;
}
