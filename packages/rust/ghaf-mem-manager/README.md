<!--
    SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Ghaf Memory Manager

`ghaf-mem-manager` monitors virtio-balloon statistics and keeps guest-visible
memory pressure within a configured window. QEMU's QMP protocol is the default
backend.

```sh
ghaf-mem-manager \
  --socket /run/microvm/app-vm.sock \
  --minimum 4294967296 \
  --maximum 12884901888
```

Select the Crosvm backend explicitly and provide the Crosvm executable used by
the VM:

```sh
ghaf-mem-manager \
  --hypervisor crosvm \
  --crosvm-binary /path/to/crosvm \
  --socket /run/microvm/app-vm.sock \
  --minimum 4294967296 \
  --maximum 12884901888
```

`--maximum` is required for Crosvm. QEMU's balloon command specifies the memory
left visible to the guest, while Crosvm's command specifies the memory reclaimed
from the guest. The Crosvm backend translates between those semantics using the
configured maximum.
