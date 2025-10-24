# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonApplication,
  lib,
  hatchling,
  uv,
  gtk3,
  gtk4,
  gobject-introspection,
  libayatana-appindicator,
  wrapGAppsHook,
  pygobject3,
}:

buildPythonApplication {
  pname = "ghaf-usb-applet";
  version = "0.1.0";
  pyproject = true;

  src = ./ghaf_usb_applet;

  build-system = [
    hatchling
    uv
  ];

  nativeBuildInputs = [
    wrapGAppsHook
    gobject-introspection
  ];

  buildInputs = [
    libayatana-appindicator
    gtk3
    gtk4
  ];

  dependencies = [
    pygobject3
  ];

  doCheck = false;
  meta = {
    description = "USB panel applet for COSMIC (GTK4)";
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "usb_applet";
  };
}
