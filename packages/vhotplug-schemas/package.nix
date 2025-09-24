# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonPackage,
  setuptools,
  wheel,
}:

buildPythonPackage {
  pname = "vhotplug_schema";
  version = "0.1.0";
  src = ./vhotplug_schemas;
  pyproject = true;

  nativeBuildInputs = [
    setuptools
    wheel
  ];
}
