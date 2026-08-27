# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
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
  wrapGAppsHook4,
  pygobject3,
  makeDesktopItem,
  copyDesktopItems,
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
    wrapGAppsHook4
    gobject-introspection
    copyDesktopItems
  ];

  desktopItems = [
    (makeDesktopItem {
      name = "ghaf.usb.settings";
      desktopName = "USB Passthrough Settings";
      comment = "Configure USB device passthrough to virtual machines";
      icon = "drive-harddisk-usb-symbolic";
      exec = "usb_settings";
      categories = [ "Settings" ];
      noDisplay = true;
    })
  ];

  buildInputs = [
    libayatana-appindicator
    gtk3
    gtk4
  ];

  dependencies = [
    pygobject3
  ];

  # TODO: Add pytest tests and enable checking
  # To enable: add pytest to nativeCheckInputs and set doCheck = true
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
