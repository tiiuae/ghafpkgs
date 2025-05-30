# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage;
    in
    {
      ghaf-artwork = callPackage ./ghaf-artwork { };
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
      ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit (inputs) crane; };
      ghaf-theme = callPackage ./ghaf-theme { };
      ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit (inputs) crane; };
      ghaf-wallpapers = callPackage ./ghaf-wallpapers { };
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
