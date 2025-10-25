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
    rev = "70f5e0565f1bc71a30e0fef1f745f4fae82c2eda";
    hash = "sha256-2yp4Rte9U8iZFrn6e5Lzrx/+GO98ZAOzwcR/xCv/7ws=";
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
