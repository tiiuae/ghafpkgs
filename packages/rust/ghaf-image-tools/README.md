<!-- SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Ghaf image tools

Shared Rust LVM/LUKS builders, retaining the existing CLI arguments and plan
JSON. Run either command with `--help`. Individual Nix outputs keep QEMU out
of LVM-only closures; `ghaf-image-tools` contains both. Native patches live in
`packages/storage`. Ghaf owns trust and platform policy, not another builder.

```sh
timeout 2h nix build .#ghaf-image-tools \
  .#ghaf-image-tools.tests.plan \
  .#ghaf-image-tools.tests.roundtrip \
  .#ghaf-image-tools.tests.clippy
```

Builds/tests use regular files: no VM, sudo, mounts, loop devices, device-mapper
activation, or impure evaluation. CI wiring is documented by this command only,
not implemented. Unit tests can also run with `cargo test --locked`.

## Contracts

- LVM modifies a disposable, initially zero-filled image. Discard it on error.
  Writes are capacity-bounded and decompressor failures propagate. Manifest
  authentication is the caller's responsibility.
- LUKS encrypts filesystem-reported data extents, including written zeroes,
  through raw slices above QEMU's LUKS driver to preserve crypto sector offsets.
  Only source holes are skipped; callers must reserve them for unused space,
  since they do **not** decrypt to guaranteed zeroes. Do not sparsify meaningful
  zeroes or mutate the source during conversion. Sizes must be sector-aligned;
  unsupported extent queries fail. Filesystems reporting all bytes as data
  remain correct but produce dense output.
- LUKS publishes by atomic rename after checking type, UUID, offset and key.
  Earlier errors preserve the original; forced termination can leave temporary
  files, and a directory-sync error after rename cannot undo publication.
  The 256-bit random construction key uses a cheap temporary KDF; that slot is
  removed, leaving the caller's key with normal cryptsetup KDF defaults.
- Tests cover bounded writes, subprocess cleanup, deterministic metadata,
  optional filesystems, zero-data preservation, sparse output and final keys.
  Rerun them after native dependency upgrades. They do not establish hardware
  boot, update, rollback or power-loss acceptance. Runtime/platform tools are
  outside this migration.
