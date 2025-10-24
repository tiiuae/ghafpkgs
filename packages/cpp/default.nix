# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ pkgs }:
let
  inherit (pkgs) callPackage;
in
{
  dbus-proxy = callPackage ./dbus-proxy { };
  ghaf-audio-control = callPackage ./ghaf-audio-control { };
}
