# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  buildPythonApplication,
  hatchling,
  pygobject3,
  dbus-python,
  gtk4,
  gobject-introspection,
  wrapGAppsHook4,
}:

buildPythonApplication rec {
  pname = "faketray";
  version = "0.1.0";

  pyproject = true;
  src = ./fake_tray;

  build-system = [ hatchling ];

  nativeBuildInputs = [
    wrapGAppsHook4
    gobject-introspection
  ];

  buildInputs = [
    gtk4
  ];

  dependencies = [
    pygobject3
    dbus-python
  ];

  doCheck = false;

  meta = with lib; {
    description = "Example GTK4 tray applet using Hatchling";
    license = licenses.asl20;
    platforms = platforms.linux;
    mainProgram = "faketray";
  };
}
