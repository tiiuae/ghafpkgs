# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  buildPythonPackage,
  setuptools,
  wheel,
}:

buildPythonPackage {
  pname = "vsock_bridge";
  version = "0.0.1";
  src = ./vsock_bridge;
  pyproject = true;

  nativeBuildInputs = [
    setuptools
    wheel
  ];
}
