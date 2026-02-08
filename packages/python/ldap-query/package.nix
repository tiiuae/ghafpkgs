# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  hatchling,
  uv,
  ldap3,
  gssapi,
  lib,
}:
buildPythonApplication {
  pname = "ldap-query";
  version = "1.0.0";
  pyproject = true;

  src = ./ldap-query;

  build-system = [
    hatchling
    uv
  ];

  dependencies = [
    ldap3
    gssapi
  ];

  # TODO: Add pytest tests and enable checking
  # To enable: add pytest to nativeCheckInputs and set doCheck = true
  doCheck = false;

  meta = {
    description = "A simple LDAP query tool";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "ldap-query";
  };
}
