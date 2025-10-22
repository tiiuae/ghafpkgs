# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
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
    rev = "8332c2e9e6ca19554eab90160ca161bf9a169a47";
    hash = "sha256-+VRRPOXJuLrWfnf0uW7BZwhp/9LMsk6HIMpxqS3vqeA=";
  };

  build-system = [
    setuptools # Required for legacy setup.py in external repo
    hatchling
    uv
  ];

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
