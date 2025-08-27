# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, self, ... }:
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
      pci-hotplug = python3Packages.callPackage ./pci-hotplug/package.nix { qemu-qmp = self.qemu_qmp; };
      qemu-qmp = python3Packages.callPackage ./qemuqmp/package.nix { };
      vhotplug = python3Packages.callPackage ./vhotplug/package.nix { qemu-qmp = self.qemu_qmp; };
      vinotify = python3Packages.callPackage ./vinotify/package.nix { };
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
