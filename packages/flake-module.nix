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
    in
    {
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
      ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit (inputs) crane; };
      ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit (inputs) crane; };
    }
    // artPackages
    // pythonPackages
    // goPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: ghafpkgs;
}
