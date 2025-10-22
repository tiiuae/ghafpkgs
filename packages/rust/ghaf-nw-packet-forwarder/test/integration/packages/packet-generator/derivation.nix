# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  setuptools,
  scapy,
  configargparse,
}:
buildPythonApplication {
  pname = "packet-gen";
  version = "1.0";
  pyproject = true;

  propagatedBuildInputs = [
    scapy
    configargparse
  ];

  src = ./.;

  build-system = [ setuptools ];
}
