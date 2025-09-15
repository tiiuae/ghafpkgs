# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  packages = with pkgs; [
    python313
    python313Packages.pygobject3
    gtk4
    gobject-introspection
    wayland
  ];

  shellHook = ''
    echo "Welcome to usb-passthrough-manager development environment!"
  '';
}
