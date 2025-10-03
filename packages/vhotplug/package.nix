# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  fetchFromGitHub,
  qemu-qmp,
  pyudev,
  psutil,
  inotify-simple,
  setuptools,
}:
buildPythonApplication {
  pname = "vhotplug";
  version = "0.1";
  pyproject = true;

  propagatedBuildInputs = [
    pyudev
    psutil
    inotify-simple
    qemu-qmp
  ];

  doCheck = false;

  src = fetchFromGitHub {
    owner = "tiiuae";
    repo = "vhotplug";
    rev = "72ce4dd15b421d7664835b43639920911065ada5";
    hash = "sha256-mI3bE0ZxNsM/6xqo53GhGU2l7td/tv+7nBLOR0rY5BY=";
  };

  build-system = [ setuptools ];

  meta = {
    description = "Virtio Hotplug";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
