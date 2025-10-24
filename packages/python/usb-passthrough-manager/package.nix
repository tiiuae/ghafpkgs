# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonApplication,
  hatchling,
  uv,
  gtk4,
  gobject-introspection,
  wrapGAppsHook,
  gsettings-desktop-schemas,
  pygobject3,
}:

buildPythonApplication {
  pname = "usb_passthrough_manager";
  version = "1.0.0";
  pyproject = true;

  src = ./usb_passthrough_manager;

  build-system = [
    hatchling
    uv
  ];

  nativeBuildInputs = [
    gobject-introspection
    wrapGAppsHook
  ];

  dependencies = [
    pygobject3
  ];

  buildInputs = [
    gtk4
    gsettings-desktop-schemas
  ];

  meta = {
    description = "Host ↔ guest VM with real user, USB passthrough management over vsock with a GTK4 (PyGObject) UI";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "usb_device_map";
  };
}
