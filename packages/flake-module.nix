# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage python3Packages;
      artPackages = callPackage ./art { inherit pkgs; };
    in
    {
      ghaf-audio-control = callPackage ./ghaf-audio-control { };
      ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit (inputs) crane; };
      ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit (inputs) crane; };
      swtpm-proxy-shim = callPackage ./swtpm-proxy-shim { };
      hotplug = python3Packages.callPackage ./hotplug/package.nix { };
      ldap-query = python3Packages.callPackage ./ldap-query/package.nix { };
      vhotplug = python3Packages.callPackage ./vhotplug/package.nix { };
      vinotify = python3Packages.callPackage ./vinotify/package.nix { };
      usb-passthrough-manager = python3Packages.callPackage ./usb-passthrough-manager/package.nix { };
    } // artPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: ghafpkgs;
}
