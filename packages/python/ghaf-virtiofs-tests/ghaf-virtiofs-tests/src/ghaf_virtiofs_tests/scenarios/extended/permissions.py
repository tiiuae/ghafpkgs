# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Permission handling test scenario.

Tests:
- F-012: File mode and ownership preserved during sync
- F-013: SUID/SGID bits stripped for security

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.permissions --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.permissions --verify
"""

import os
import stat
import time

from ...context import TEST_FILE_PREFIX, TestContext

# F-012: Permission preservation
PERM_FILE = TEST_FILE_PREFIX + "perm_test.txt"
PERM_MODE = 0o640  # rw-r-----

# F-013: SUID/SGID stripping
SUID_FILE = TEST_FILE_PREFIX + "suid_test.txt"
SUID_MODE = 0o4755  # rwsr-xr-x (SUID set)
SGID_FILE = TEST_FILE_PREFIX + "sgid_test.txt"
SGID_MODE = 0o2755  # rwxr-sr-x (SGID set)

TEST_CONTENT = b"Permission test content"


class PermissionsScenario:
    """Test file permission handling."""

    def write(self, ctx: TestContext) -> None:
        """Create files with specific permissions."""

        # F-012: Create file with specific mode
        print(f"[F-012] Creating file with mode {oct(PERM_MODE)}...")
        perm_file = ctx.path / PERM_FILE
        perm_file.write_bytes(TEST_CONTENT)
        os.chmod(perm_file, PERM_MODE)
        actual_mode = stat.S_IMODE(perm_file.stat().st_mode)
        print(f"  Created: {PERM_FILE} with mode {oct(actual_mode)}")

        # F-013: Create file with SUID bit
        print(f"\n[F-013] Creating file with SUID mode {oct(SUID_MODE)}...")
        suid_file = ctx.path / SUID_FILE
        suid_file.write_bytes(TEST_CONTENT)
        try:
            os.chmod(suid_file, SUID_MODE)
            actual_mode = stat.S_IMODE(suid_file.stat().st_mode)
            print(f"  Created: {SUID_FILE} with mode {oct(actual_mode)}")
        except PermissionError as e:
            print(f"  Cannot set SUID (expected in VM): {e}")

        # F-013: Create file with SGID bit
        print(f"\n[F-013] Creating file with SGID mode {oct(SGID_MODE)}...")
        sgid_file = ctx.path / SGID_FILE
        sgid_file.write_bytes(TEST_CONTENT)
        try:
            os.chmod(sgid_file, SGID_MODE)
            actual_mode = stat.S_IMODE(sgid_file.stat().st_mode)
            print(f"  Created: {SGID_FILE} with mode {oct(actual_mode)}")
        except PermissionError as e:
            print(f"  Cannot set SGID (expected in VM): {e}")

        print("\nPermission test files created")

    def verify(self, ctx: TestContext) -> None:
        """Verify permission handling (run on host)."""

        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Find producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        if not producer_dirs:
            raise RuntimeError(f"No producer directories in {share_dir}")

        print(f"Found producers: {list(producer_dirs.keys())}")
        start_time = time.monotonic()

        # Wait for permission test file
        print(f"\nWaiting for {PERM_FILE}...")
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / PERM_FILE).exists():
                    source_dir = path
                    print(f"  Found in producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {PERM_FILE}")
            time.sleep(0.1)

        results = {
            "perm_preserved": False,
            "suid_stripped": False,
            "sgid_stripped": False,
        }

        # === F-012: Check permission preservation ===
        print("\n[F-012] Checking permission preservation...")
        perm_file = source_dir / PERM_FILE
        mode = stat.S_IMODE(perm_file.stat().st_mode)
        # Check only permission bits (ignore SUID/SGID/sticky)
        perm_bits = mode & 0o777

        if perm_bits == PERM_MODE:
            results["perm_preserved"] = True
            print(f"  OK: Mode preserved as {oct(perm_bits)}")
        else:
            print(f"  FAIL: Mode is {oct(perm_bits)} (expected {oct(PERM_MODE)})")
            results["perm_preserved"] = False

        # === F-013: Check SUID stripped ===
        print("\n[F-013] Checking SUID/SGID stripping...")

        # Wait for SUID file
        suid_file = source_dir / SUID_FILE
        timeout = time.monotonic() + 30.0
        while not suid_file.exists() and time.monotonic() < timeout:
            time.sleep(0.1)

        if suid_file.exists():
            mode = stat.S_IMODE(suid_file.stat().st_mode)
            has_suid = bool(mode & stat.S_ISUID)

            if not has_suid:
                results["suid_stripped"] = True
                print(f"  OK: SUID stripped, mode is {oct(mode)}")
            else:
                raise AssertionError(f"SUID still set, mode is {oct(mode)}")
        else:
            raise AssertionError(f"{SUID_FILE} not found")

        # Wait for SGID file
        sgid_file = source_dir / SGID_FILE
        timeout = time.monotonic() + 30.0
        while not sgid_file.exists() and time.monotonic() < timeout:
            time.sleep(0.1)

        if sgid_file.exists():
            mode = stat.S_IMODE(sgid_file.stat().st_mode)
            has_sgid = bool(mode & stat.S_ISGID)

            if not has_sgid:
                results["sgid_stripped"] = True
                print(f"  OK: SGID stripped, mode is {oct(mode)}")
            else:
                raise AssertionError(f"SGID still set, mode is {oct(mode)}")
        else:
            raise AssertionError(f"{SGID_FILE} not found")

        # Summary
        print("\n" + "=" * 50)
        print("PERMISSION TEST RESULTS")
        print("=" * 50)
        print(
            f"  F-012 Permission preserved: {'PASS' if results['perm_preserved'] else 'FAIL'}"
        )
        print(
            f"  F-013 SUID stripped: {'PASS' if results['suid_stripped'] else 'FAIL'}"
        )
        print(
            f"  F-013 SGID stripped: {'PASS' if results['sgid_stripped'] else 'FAIL'}"
        )

        if all(results.values()):
            print("\nPermission tests passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"Permission tests failed: {failed}")
