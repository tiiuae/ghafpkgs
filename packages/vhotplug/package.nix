# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  qemu-qmp,
  pyudev,
  psutil,
  inotify-simple,
  setuptools,
  vsock-bridge,
}:
buildPythonApplication {
  pname = "vhotplug";
  version = "0.1.0";
  pyproject = true;

  propagatedBuildInputs = [
    pyudev
    psutil
    inotify-simple
    qemu-qmp
    vsock-bridge
  ];

  doCheck = false;

  src = ./vhotplug;

  build-system = [ setuptools ];

  meta = {
    description = "Virtio Hotplug";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
