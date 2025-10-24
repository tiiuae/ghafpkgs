# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  packages = with pkgs; [
    python313
    python313Packages.pygobject3
    python313Packages.virtualenv
    gtk3
    gtk4
    gobject-introspection
    libayatana-appindicator
  ];

  shellHook = ''
    VENV_DIR=".venv"

    if [ ! -d "$VENV_DIR" ]; then
      echo "Creating Python venv in $VENV_DIR..."
      python -m venv $VENV_DIR
    fi

    echo "Activating virtual environment..."
    source $VENV_DIR/bin/activate

    echo "Python version: $(python --version)"
    echo "Pip version: $(pip --version)"
  '';
}
