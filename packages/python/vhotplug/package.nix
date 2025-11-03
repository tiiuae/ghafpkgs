# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  fetchFromGitHub,
  qemu-qmp,
  pyudev,
  psutil,
  inotify-simple,
  setuptools, # Required for legacy setup.py in external repo
  hatchling,
  uv,
  lib,
}:
buildPythonApplication {
  pname = "vhotplug";
  version = "1.0.0";
  pyproject = true;

  src = fetchFromGitHub {
    owner = "tiiuae";
    repo = "vhotplug";
    rev = "fc9da0c45d7ab102c428134f4cb898c728194395";
    hash = "sha256-qeTydDm4UHTbirzbsiE7TUMjo8YeU98qcBFfCQpRG5U=";
  };

  build-system = [
    setuptools # Required for legacy setup.py in external repo
    hatchling
    uv
  ];

  dependencies = [
    pyudev
    psutil
    inotify-simple
    qemu-qmp
  ];

  doCheck = false;

  meta = {
    description = "Virtio Hotplug - Virtual device hotplug management for QEMU";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "vhotplug";
  };
}
