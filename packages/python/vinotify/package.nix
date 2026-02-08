# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  inotify-simple,
  hatchling,
  uv,
  lib,
}:
buildPythonApplication {
  pname = "vinotify";
  version = "1.0.0";
  pyproject = true;

  src = ./vinotify;

  build-system = [
    hatchling
    uv
  ];

  dependencies = [
    inotify-simple
  ];

  # TODO: Add pytest tests and enable checking
  # To enable: add pytest to nativeCheckInputs and set doCheck = true
  doCheck = false;

  meta = {
    description = "Virtual machine file system notification service using inotify";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "vinotify";
  };
}
