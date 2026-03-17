# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  hatchling,
  uv,
  lib,
}:
buildPythonApplication {
  pname = "ghaf-virtiofs-tests";
  version = "0.1.0";
  pyproject = true;

  src = ./ghaf-virtiofs-tests;

  build-system = [
    hatchling
    uv
  ];

  dependencies = [ ];

  doCheck = false;

  meta = {
    description = "Integration tests for ghaf-virtiofs-tools";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "ghaf-virtiofs-test";
  };
}
