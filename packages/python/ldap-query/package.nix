# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
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
  build-system = [
    hatchling
    uv
  ];

  src = ./ldap-query;

  propagatedBuildInputs = [
    ldap3
    gssapi
  ];

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
