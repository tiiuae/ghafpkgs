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
    rev = "8332c2e9e6ca19554eab90160ca161bf9a169a47";
    hash = "sha256-+VRRPOXJuLrWfnf0uW7BZwhp/9LMsk6HIMpxqS3vqeA=";
  };

  build-system = [ setuptools ];

  meta = {
    description = "Virtio Hotplug";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "vhotplug";
  };
}
