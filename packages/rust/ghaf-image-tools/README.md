<!-- SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Ghaf image tools

Shared Rust LVM/LUKS builders. LVM takes `--manifest PATH` and resolves payloads
beside it; the plan JSON is unchanged. The VG is `pool`, and the LUKS header
reservation is 32 MiB. Run either command with `--help`. Individual outputs keep QEMU out
of LVM-only closures; `ghaf-image-tools` contains both. LVM metadata is written
directly in Rust; encryption uses unpatched QEMU and cryptsetup.
Ghaf owns trust and platform policy, not another builder.

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
  Writes are capacity-bounded, unpacked lengths must match the manifest, and
  decompressor failures propagate. Manifest
  authentication is the caller's responsibility.
- The creation-only writer emits LVM2 format-text with one PV, one metadata
  area and contiguous LVs (4 MiB extents, 5 MiB data offset). It does not edit
  existing volumes. Stock LVM owns activation and subsequent metadata changes.
- LUKS uses `qemu-img map -f raw --output=json` to select data extents,
  including written zeroes,
  through raw slices above QEMU's LUKS driver to preserve crypto sector offsets.
  Only source holes are skipped; callers must reserve them for unused space,
  since they do **not** decrypt to guaranteed zeroes. Do not sparsify meaningful
  zeroes or mutate the source during conversion. Sizes must be sector-aligned;
  failed or invalid extent maps abort. Filesystems reporting all bytes as data
  remain correct but produce dense output. UUID validation is delegated to
  cryptsetup.
- LUKS publishes by atomic rename after checking type, UUID, offset and key.
  Earlier errors preserve the original; forced termination can leave temporary
  files, and a directory-sync error after rename cannot undo publication.
  A temporary LUKS1 header lets QEMU encrypt the data. Stock cryptsetup then
  recreates the header as LUKS2 with the same random volume key, cipher, sector
  size and offset, but only the caller's passphrase and default KDF. Temporary
  key files are private and removed on normal exit.
- Tests cover bounded writes, subprocess cleanup, deterministic metadata,
  optional filesystems, zero-data preservation, sparse output and final keys.
  Rerun them after native dependency upgrades.
