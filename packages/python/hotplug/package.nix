# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  qemu-qmp,
  systemd-python,
  hatchling,
  uv,
}:
buildPythonApplication {
  pname = "hotplug";
  version = "1.0.0";
  pyproject = true;

  # Use the hotplug subdirectory as source, where package.nix is located alongside pyproject.toml
  src = ./hotplug;

  build-system = [
    hatchling
    uv
  ];

  dependencies = [
    qemu-qmp
    systemd-python
  ];

  doCheck = false;

  meta = {
    description = "Qemu hotplug helper for PCI and USB devices";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
