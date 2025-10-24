# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonApplication,
  setuptools,
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
  src = ./ghaf_usb_applet;
  pyproject = true;

  nativeBuildInputs = [
    setuptools
    wrapGAppsHook
    gobject-introspection
  ];

  buildInputs = [
    libayatana-appindicator
    gtk3
    gtk4
  ];
  propagatedBuildInputs = [
    pygobject3
  ];
}
