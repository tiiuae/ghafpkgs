# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ pkgs }:
let
  inherit (pkgs) python3Packages;
in
{
  hotplug = python3Packages.callPackage ./hotplug/package.nix { };
  ldap-query = python3Packages.callPackage ./ldap-query/package.nix { };
  vhotplug = python3Packages.callPackage ./vhotplug/package.nix { };
  vinotify = python3Packages.callPackage ./vinotify/package.nix { };
  ghaf-usb-applet = python3Packages.callPackage ./ghaf-usb-applet/package.nix { };
  gps-websock = python3Packages.callPackage ./gps-websock/package.nix { };
}
