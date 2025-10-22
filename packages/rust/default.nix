# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ pkgs, crane }:
let
  inherit (pkgs) callPackage;
in
{
  ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit crane; };
  ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit crane; };
}
