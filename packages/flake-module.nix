# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage python3Packages;
    in
    {
      ghaf-artwork = callPackage ./ghaf-artwork { };
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
      ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit (inputs) crane; };
      ghaf-theme = callPackage ./ghaf-theme { };
      ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit (inputs) crane; };
      ghaf-wallpapers = callPackage ./ghaf-wallpapers { };
      swtpm-proxy-shim = callPackage ./swtpm-proxy-shim { };
      hotplug = python3Packages.callPackage ./hotplug/package.nix { };
      ldap-query = python3Packages.callPackage ./ldap-query/package.nix { };
      vhotplug = python3Packages.callPackage ./vhotplug/package.nix { };
      vinotify = python3Packages.callPackage ./vinotify/package.nix { };
      usb-passthrough-manager = python3Packages.callPackage ./usb-passthrough-manager/package.nix { };
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
