# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonApplication,
  hatchling,
  uv,
  websockets,
  lib,
}:

buildPythonApplication {
  pname = "gpswebsock";
  version = "1.0.0";
  pyproject = true;

  src = ./gps-websock;

  build-system = [
    hatchling
    uv
  ];

  dependencies = [
    websockets
  ];

  # TODO: Add pytest tests and enable checking
  # To enable: add pytest to nativeCheckInputs and set doCheck = true
  doCheck = false;

  meta = {
    description = "GPS endpoint exposed over WebSocket";
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "gpswebsock";
  };
}
