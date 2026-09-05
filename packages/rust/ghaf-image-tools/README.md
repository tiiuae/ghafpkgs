<!-- SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Ghaf image tools

Shared Rust library and regular-file image builders. These replace the shared
LVM/LUKS shell implementations from Ghaf's secure A/B stack, preserving their
command names and arguments. Ghaf retains NixOS options, trust configuration,
and platform layout policy; this package owns image construction mechanics.
Platform orchestration and runtime update tools are not migrated in this slice.

## Commands

- `ghaf-initialize-verity-lvm --update-dir DIR --image FILE --root-size-mib N
  --verity-size-mib N`: initialize an existing, zero-filled sparse file with the
  manifest's root/verity payloads. `--print-plan` emits the existing JSON plan
  without opening or modifying the target image. Optional inactive slots, swap,
  persist, and VG name retain their previous flags.
- `ghaf-wrap-luks-image --image FILE --uuid UUID --key-file FILE`: construct a
  LUKS2 wrapper with a 32 MiB header by default, then replace the plaintext file.
  `--header-size-mib` overrides that reservation (minimum 16 MiB).

Use the packages with the same names for individual commands. Their separate
Nix outputs keep QEMU out of LVM-only consumers. `ghaf-image-tools` contains both.
Compiled wrappers supply native dependencies and pin the offline executables;
there is no shell implementation behind the commands.

## Build and test

```sh
timeout 2h nix build .#ghaf-image-tools \
  .#ghaf-image-tools.tests.plan \
  .#ghaf-image-tools.tests.roundtrip \
  .#ghaf-image-tools.tests.clippy
```

The builds and tests use only regular files and userspace processes. No VM,
sudo, mount, loop device, device-mapper activation, or impure evaluation is
required. The native dependencies live in `packages/storage`: an offline-only
LVM2 build and cryptsetup's regular-file header conversion patch. This does not
replace those projects' metadata formats or cryptographic implementations.

For development, `cargo test --locked` and `cargo clippy --locked --all-targets
-- -D warnings` exercise the Rust library. Tests cover bounded writes, malformed
extent reports, checked arithmetic, producer failures, cleanup, and publication.
Nix integration tests additionally check stock-LVM-readable deterministic
metadata, optional filesystems, overlong streams, and LUKS decryption, including
written zero blocks, mixed sectors, distant extents, sparse output, and final
key-slot KDF settings.

## Correctness boundaries

- The LVM initializer accepts a build artifact manifest, not an authenticated
  update request. Signature/hash verification remains the caller's policy.
  Writes are bounded by the declared LV capacity even if decompression produces
  too much data; producer failures are checked and subprocesses reaped.
- LVM construction is in-place in a disposable, initially zero-filled image.
  Failure can leave partial metadata/payloads: discard that image. Sparse
  filesystem copies rely on the initial zero-filled regions.
- LUKS construction uses a private sibling file and a temporary construction
  key. It checks the final type, UUID, payload offset, and supplied key before
  atomic replacement. Errors before replacement preserve the original. Forced
  termination may leave temporary files; an error syncing the parent directory
  after replacement cannot undo publication.
- LUKS copying uses `SEEK_DATA`/`SEEK_HOLE`, not zero-byte detection. QEMU copies
  each data extent with sparsification disabled through raw slices above the
  LUKS driver, preserving written zeroes and absolute crypto sector offsets.
  Only filesystem-reported holes are skipped. The caller must ensure those
  holes are unused space: they do **not** decrypt to guaranteed zeroes. Do not
  mutate the source while wrapping it or sparsify meaningful zero data first.
  Unsupported extent queries fail; filesystems reporting the whole file as data
  remain correct but produce dense output. Image sizes must be sector-aligned.
- The construction passphrase has 256 random bits and uses a cheap temporary
  PBKDF2 slot to avoid an expensive KDF for every extent. That slot is removed
  before publication; the caller's retained key uses cryptsetup's normal KDF
  defaults. The integration test verifies the final slot, not just unlock.
- These are offline image-building tools, not runtime block-device managers.
  Native dependency upgrades require rerunning the integration tests. Passing
  them does not establish boot, rollback, power-loss, or hardware acceptance.

CI wiring is deliberately not added here. A future CI job can invoke the same
pure test command above; signing trust and private keys are outside this package.
