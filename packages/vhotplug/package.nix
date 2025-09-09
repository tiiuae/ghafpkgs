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
    rev = "e17360469a2c787526b251a0fa75cbec7e7ae6b3";
    hash = "sha256-l66Skt5T7qys0GSjXfGFftTydH3SSFK1bftMBxVt+lQ=";
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
