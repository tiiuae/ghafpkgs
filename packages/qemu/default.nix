# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage }:
let
  ghaf-x86-qemu = callPackage ./base/package.nix {
    packageName = "ghaf-x86-qemu";
    withX86Acpi = true;
  };
  ghaf-nvidia-qemu = callPackage ./base/package.nix {
    packageName = "ghaf-nvidia-qemu";
  };
  ghaf-nvidia-qemu-bpmp = callPackage ./ghaf-nvidia-qemu-bpmp/package.nix {
    inherit ghaf-nvidia-qemu;
  };
in
{
  x86QemuPackages = { inherit ghaf-x86-qemu; };
  nvidiaQemuPackages = {
    inherit ghaf-nvidia-qemu ghaf-nvidia-qemu-bpmp;
    ghaf-nvidia-qemu-bpmp-gpu = callPackage ./ghaf-nvidia-qemu-bpmp-gpu/package.nix {
      inherit ghaf-nvidia-qemu-bpmp;
    };
  };
}
