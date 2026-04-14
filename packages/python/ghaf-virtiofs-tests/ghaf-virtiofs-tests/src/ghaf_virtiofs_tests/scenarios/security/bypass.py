# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Security bypass attempt test scenario.

Tests symlink/hardlink attacks and special file handling.
Gate MUST reject all symlinks to prevent cross-VM attacks.

Symlink Attacks:
- SEC-009: Symlink to file (/etc/passwd - cross-VM data leak)
- SEC-010: Symlink replace (replace regular file with symlink)
- SEC-011: Symlink to device (/dev/zero - infinite read, /dev/sda - disk access)
- SEC-012: Symlink loop (a->b->a - DoS via infinite loop)
- SEC-013: Symlink directory escape (dir->/tmp, write through it)

Hardlink Attack:
- SEC-014: Hardlink (propagation as separate file with different inode)

Special Files:
- SEC-015: FIFO/Socket (named pipe - should be blocked)
- SEC-016: Device file (block/char device - should be blocked)

Filename Tests:
- SEC-017: Dotdot in name (foo..bar - should be allowed)
- SEC-018: Null byte in name (should be blocked)

Other:
- SEC-019: Race create-delete (stress test)
- SEC-020: Ignore pattern escape (rename from .crdownload)

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s security.bypass --write

    # Host verifier:
    ghaf-virtiofs-test run /mnt/basePath -s security.bypass --verify
"""

import os
import stat
import time

from ...context import TEST_FILE_PREFIX, TestContext

# File prefix for security bypass tests
SEC_PREFIX = TEST_FILE_PREFIX + "sec"


class BypassScenario:
    """Test security bypass attempt handling."""

    def write(self, ctx: TestContext) -> None:
        """Run bypass attempt tests from VM."""
        results = {}

        # === SYMLINK ATTACKS ===

        # SEC-009: Symlink to sensitive file
        # Threat: If propagated, victim VM reads its own /etc/passwd
        print("[SEC-009] Symlink to file (/etc/passwd)...")
        try:
            symlink_file = ctx.path / f"{SEC_PREFIX}009_passwd_link"
            symlink_file.symlink_to("/etc/passwd")
            print("  Created symlink -> /etc/passwd")
            results["sec009"] = True
        except OSError as e:
            print(f"  Blocked locally: {e}")
            results["sec009"] = True

        # SEC-010: Symlink replace
        # Threat: Replace legitimate file with symlink after initial sync
        print("\n[SEC-010] Symlink replace...")
        try:
            normal_file = ctx.path / f"{SEC_PREFIX}010_replaced"
            normal_file.write_bytes(b"normal content")
            time.sleep(0.5)
            normal_file.unlink()
            normal_file.symlink_to("/etc/passwd")
            print("  Replaced file with symlink")
            results["sec010"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec010"] = True

        # SEC-011: Symlink to device
        # Threat: /dev/zero causes infinite read, /dev/sda allows disk access
        print("\n[SEC-011] Symlink to device (/dev/zero)...")
        try:
            dev_symlink = ctx.path / f"{SEC_PREFIX}011_dev_link"
            dev_symlink.symlink_to("/dev/zero")
            print("  Created symlink -> /dev/zero")
            results["sec011"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec011"] = True

        # SEC-012: Symlink loop
        # Threat: Infinite loop in gate processing (DoS)
        print("\n[SEC-012] Symlink loop...")
        try:
            loop_a = ctx.path / f"{SEC_PREFIX}012_loop_a"
            loop_b = ctx.path / f"{SEC_PREFIX}012_loop_b"
            loop_a.symlink_to(f"{SEC_PREFIX}012_loop_b")
            loop_b.symlink_to(f"{SEC_PREFIX}012_loop_a")
            print("  Created symlink loop (a->b->a)")
            results["sec012"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec012"] = True

        # SEC-013: Symlink directory escape
        # Threat: Create dir symlink, write through it to escape share
        print("\n[SEC-013] Symlink directory escape...")
        try:
            symdir = ctx.path / f"{SEC_PREFIX}013_symdir"
            symdir.symlink_to("/tmp")
            escape_file = symdir / f"{SEC_PREFIX}013_escaped.txt"
            try:
                escape_file.write_bytes(b"escaped content")
                print("  Created symlink dir -> /tmp, wrote through it")
            except (OSError, PermissionError):
                print("  Symlink dir created, write blocked")
            results["sec013"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec013"] = True

        # === HARDLINK ATTACK ===

        # SEC-014: Hardlink
        # Threat: Same inode could bypass per-file scanning
        print("\n[SEC-014] Hardlink attack...")
        try:
            original = ctx.path / f"{SEC_PREFIX}014_original.txt"
            original.write_bytes(b"original content")
            hardlink = ctx.path / f"{SEC_PREFIX}014_hardlink.txt"
            os.link(original, hardlink)
            print("  Created hardlink to file")
            results["sec014"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec014"] = True

        # === SPECIAL FILES ===

        # SEC-015: FIFO/Socket
        print("\n[SEC-015] FIFO creation...")
        try:
            fifo_path = ctx.path / f"{SEC_PREFIX}015_fifo"
            os.mkfifo(fifo_path)
            print("  Created FIFO")
            results["sec015"] = True
        except OSError as e:
            print(f"  Blocked: {e}")
            results["sec015"] = True

        # SEC-016: Device file
        print("\n[SEC-016] Device file creation...")
        try:
            dev_path = ctx.path / f"{SEC_PREFIX}016_device"
            os.mknod(dev_path, stat.S_IFBLK | 0o666, os.makedev(1, 3))
            print("  Created device file")
            results["sec016"] = True
        except (OSError, PermissionError) as e:
            print(f"  Blocked: {e}")
            results["sec016"] = True

        # === FILENAME TESTS ===

        # SEC-017: Dotdot in name (should be ALLOWED)
        print("\n[SEC-017] Dotdot in filename (foo..bar)...")
        dotdot_file = ctx.path / f"{SEC_PREFIX}017_foo..bar.txt"
        dotdot_file.write_bytes(b"dotdot in name")
        print("  OK: foo..bar allowed (not traversal)")
        results["sec017"] = True

        # SEC-018: Null byte in name
        print("\n[SEC-018] Null byte in filename...")
        try:
            null_file = ctx.path / f"{SEC_PREFIX}018_null\x00byte.txt"
            null_file.write_bytes(b"null byte")
            raise AssertionError("Null byte accepted in filename")
        except (OSError, ValueError) as e:
            print(f"  OK: Blocked - {e}")
            results["sec018"] = True

        # === OTHER ===

        # SEC-019: Race create-delete
        print("\n[SEC-019] Race create-delete...")
        try:
            for i in range(100):
                race_file = ctx.path / f"{SEC_PREFIX}019_race_{i}.txt"
                race_file.write_bytes(b"race")
                try:
                    race_file.unlink()
                except FileNotFoundError:
                    pass
            print("  OK: Race handled")
            results["sec019"] = True
        except Exception as e:
            print(f"  Error: {e}")
            results["sec019"] = False

        # SEC-020: Ignore pattern escape
        print("\n[SEC-020] Ignore pattern escape...")
        try:
            ignored = ctx.path / f"{SEC_PREFIX}020_test.crdownload"
            ignored.write_bytes(b"ignored content")
            time.sleep(0.5)
            final = ctx.path / f"{SEC_PREFIX}020_test.txt"
            ignored.rename(final)
            print("  Renamed from ignored extension")
            results["sec020"] = True
        except Exception as e:
            print(f"  Error: {e}")
            results["sec020"] = False

        # Summary
        print("\n" + "=" * 50)
        print("BYPASS TEST RESULTS (VM)")
        print("=" * 50)
        for test, passed in results.items():
            print(f"  {test}: {'PASS' if passed else 'FAIL'}")
        print(f"\n{sum(results.values())}/{len(results)} tests passed")

    def verify(self, ctx: TestContext) -> None:
        """Verify bypass attempts were blocked (run on host)."""
        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        print(f"Found producers: {list(producer_dirs.keys())}")

        print("\nWaiting for test files...")
        time.sleep(10.0)

        results = {}

        # === FIND SOURCE PRODUCER ===
        # The source is the producer where symlinks exist (where write was run)
        source_producer = None
        for name, path in producer_dirs.items():
            if (path / f"{SEC_PREFIX}009_passwd_link").exists():
                source_producer = name
                print(f"Source producer: {name}")
                break

        if not source_producer:
            raise AssertionError(
                "No symlinks found in any producer - test may not have run"
            )

        # Find other producers for verification
        other_producers = {
            n: p for n, p in producer_dirs.items() if n != source_producer
        }

        if not other_producers:
            raise RuntimeError(
                "Bypass test: No other producers to verify propagation (need at least 2)"
            )

        # === CHECK SYMLINKS NOT PROPAGATED ===
        print("\n=== SYMLINK PROPAGATION CHECKS ===")

        symlink_tests = [
            (f"{SEC_PREFIX}009_passwd_link", "SEC-009 (file symlink)"),
            (f"{SEC_PREFIX}010_replaced", "SEC-010 (replaced symlink)"),
            (f"{SEC_PREFIX}011_dev_link", "SEC-011 (device symlink)"),
            (f"{SEC_PREFIX}012_loop_a", "SEC-012 (symlink loop)"),
            (f"{SEC_PREFIX}013_symdir", "SEC-013 (dir symlink)"),
        ]

        for filename, test_name in symlink_tests:
            propagated = False

            # Check other producers (not source)
            for name, path in other_producers.items():
                check_path = path / filename
                if check_path.exists() and check_path.is_symlink():
                    print(f"  {test_name}: CRITICAL - Symlink propagated to {name}!")
                    propagated = True

            if not propagated:
                print(f"  {test_name}: OK - Not propagated")
            results[filename] = not propagated

        # === CHECK PROPAGATION TO OTHER PRODUCERS ===
        print("\n=== PROPAGATION CHECKS ===")

        # SEC-014: Hardlink propagation check
        # Unlike symlinks, hardlinks are regular files and SHOULD propagate.
        # The security check is that they become separate files (different inodes)
        # on the destination, preventing inode-based scan bypass across VMs.
        source_path = producer_dirs[source_producer]
        source_original = source_path / f"{SEC_PREFIX}014_original.txt"
        source_hardlink = source_path / f"{SEC_PREFIX}014_hardlink.txt"

        # Wait for source files to be created
        timeout = time.monotonic() + 30.0
        while not source_original.exists() and time.monotonic() < timeout:
            time.sleep(0.1)
        if not source_original.exists():
            raise AssertionError("SEC-014: Original file not created in source")

        while not source_hardlink.exists() and time.monotonic() < timeout:
            time.sleep(0.1)
        if not source_hardlink.exists():
            raise AssertionError("SEC-014: Hardlink not created in source")

        print(f"  SEC-014: Source files exist in {source_producer}")

        for name, path in other_producers.items():
            target_original = path / f"{SEC_PREFIX}014_original.txt"
            target_hardlink = path / f"{SEC_PREFIX}014_hardlink.txt"

            # Wait for original to propagate (hardlinks are regular files)
            timeout = time.monotonic() + 30.0
            while not target_original.exists() and time.monotonic() < timeout:
                time.sleep(0.1)

            if not target_original.exists():
                raise AssertionError(
                    f"SEC-014: Original file not propagated to {name} (timeout)"
                )

            # If hardlink also propagated, verify it has different inode
            # (proving it was copied, not hardlinked across VMs)
            if target_hardlink.exists():
                orig_inode = target_original.stat().st_ino
                link_inode = target_hardlink.stat().st_ino
                if orig_inode == link_inode:
                    raise AssertionError(
                        f"SEC-014: Hardlink shares inode in {name} "
                        f"(security issue: same inode {orig_inode})"
                    )
                print(f"  SEC-014: OK - Files have different inodes in {name}")
            else:
                print(f"  SEC-014: OK - Original propagated to {name}")

        results["sec014"] = True

        # SEC-015: FIFO should NOT propagate
        for name, path in other_producers.items():
            if (path / f"{SEC_PREFIX}015_fifo").exists():
                raise AssertionError(f"SEC-015: FIFO propagated to {name}")
        print("  SEC-015: OK - FIFO not propagated")
        results["sec015"] = True

        # SEC-016: Device should NOT propagate
        for name, path in other_producers.items():
            if (path / f"{SEC_PREFIX}016_device").exists():
                raise AssertionError(f"SEC-016: Device propagated to {name}")
        print("  SEC-016: OK - Device not propagated")
        results["sec016"] = True

        # SEC-017: Dotdot file SHOULD propagate
        for name, path in other_producers.items():
            target = path / f"{SEC_PREFIX}017_foo..bar.txt"
            timeout = time.monotonic() + 30.0
            while not target.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target.exists():
                raise AssertionError(f"SEC-017: foo..bar not propagated to {name}")
        print("  SEC-017: OK - foo..bar synced")
        results["sec017"] = True

        # SEC-019: Race files should NOT propagate (deleted before sync)
        for name, path in other_producers.items():
            race_files = list(path.glob(f"{SEC_PREFIX}019_race_*.txt"))
            if race_files:
                raise AssertionError(
                    f"SEC-019: {len(race_files)} race files propagated to {name}"
                )
        print("  SEC-019: OK - No race files propagated")
        results["sec019"] = True

        # SEC-020: Renamed file SHOULD propagate
        for name, path in other_producers.items():
            target = path / f"{SEC_PREFIX}020_test.txt"
            timeout = time.monotonic() + 30.0
            while not target.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target.exists():
                raise AssertionError(f"SEC-020: Renamed file not propagated to {name}")
        print("  SEC-020: OK - Renamed file synced")
        results["sec020"] = True

        # Summary
        print("\n" + "=" * 50)
        print("BYPASS VERIFICATION RESULTS (HOST)")
        print("=" * 50)
        for test, passed in results.items():
            status = "PASS" if passed else "FAIL"
            print(f"  {test}: {status}")

        passed_count = sum(results.values())
        total = len(results)
        print(f"\n{passed_count}/{total} verifications passed")

        # Fail if any symlink propagated (critical security issue)
        symlink_failures = [k for k in symlink_tests if not results.get(k[0], True)]
        if symlink_failures:
            raise AssertionError(f"CRITICAL: Symlinks propagated - {symlink_failures}")
