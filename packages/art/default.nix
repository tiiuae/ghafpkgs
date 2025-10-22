# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ pkgs }:
let
  inherit (pkgs) callPackage;
in
{
  ghaf-artwork = callPackage ./ghaf-artwork { };
  ghaf-theme = callPackage ./ghaf-theme { };
  ghaf-wallpapers = callPackage ./ghaf-wallpapers { };
}