# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Symlink rejection test (extended).

Tests:
- F-017: Symlinks are not followed and not synced

The gate should ignore symlinks for security reasons - following symlinks
could allow escaping the share directory.

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.symlink --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.symlink --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_CONTENT = b"Real file content"
REAL_FILE = TEST_FILE_PREFIX + "symlink_real_file.txt"
SYMLINK_FILE = TEST_FILE_PREFIX + "symlink_to_file.txt"
SYMLINK_DIR = TEST_FILE_PREFIX + "symlink_to_dir"
REAL_DIR = TEST_FILE_PREFIX + "symlink_real_dir"
DIR_FILE = TEST_FILE_PREFIX + "file_in_dir.txt"


class SymlinkScenario:
    """Test symlink rejection."""

    def write(self, ctx: TestContext) -> None:
        """Create real files and symlinks."""

        # Create real file
        print(f"Creating real file: {REAL_FILE}")
        ctx.create_test_file(REAL_FILE, content=TEST_CONTENT)

        # Create real directory with file
        print(f"Creating real directory: {REAL_DIR}/")
        real_dir = ctx.path / REAL_DIR
        real_dir.mkdir(parents=True, exist_ok=True)
        (real_dir / DIR_FILE).write_bytes(TEST_CONTENT)

        # Create symlink to file
        print(f"Creating symlink to file: {SYMLINK_FILE} -> {REAL_FILE}")
        symlink_file = ctx.path / SYMLINK_FILE
        try:
            symlink_file.symlink_to(REAL_FILE)
            print("  Symlink created")
        except OSError as e:
            print(f"  Failed to create symlink: {e}")

        # Create symlink to directory
        print(f"Creating symlink to directory: {SYMLINK_DIR} -> {REAL_DIR}")
        symlink_dir = ctx.path / SYMLINK_DIR
        try:
            symlink_dir.symlink_to(REAL_DIR)
            print("  Symlink created")
        except OSError as e:
            print(f"  Failed to create symlink: {e}")

        print("\nSymlink test: real files and symlinks created")
        print("Expected: real files sync, symlinks do NOT sync")

    def verify(self, ctx: TestContext) -> None:
        """Verify real files sync but symlinks don't (run on host)."""

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

        # Wait for real file to detect source producer
        print(f"\nWaiting for real file: {REAL_FILE}")
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / REAL_FILE).exists():
                    source_dir = path
                    print(f"  Found source producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {REAL_FILE}")
            time.sleep(0.1)

        results = {
            "real_file": False,
            "real_dir": False,
            "symlink_file_rejected": False,
            "symlink_dir_rejected": False,
        }

        # Find other producers (not source) to check propagation
        other_producers = {n: p for n, p in producer_dirs.items() if p != source_dir}

        if not other_producers:
            raise RuntimeError(
                "Symlink test: No other producers to verify propagation (need at least 2)"
            )

        # Check real files propagated to other producers
        print("\n[Real Files] Checking real files propagated...")
        for name, path in other_producers.items():
            # Check real file
            target_file = path / REAL_FILE
            timeout = time.monotonic() + 30.0
            while not target_file.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_file.exists():
                raise AssertionError(f"Real file not propagated to {name}")
            if target_file.is_symlink():
                raise AssertionError(f"Real file is symlink in {name}")
            content = target_file.read_bytes()
            if content != TEST_CONTENT:
                raise AssertionError(f"Real file content mismatch in {name}")
            print(f"  OK: {REAL_FILE} propagated to {name}")

            # Check real directory with file
            target_dir_file = path / REAL_DIR / DIR_FILE
            timeout = time.monotonic() + 30.0
            while not target_dir_file.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_dir_file.exists():
                raise AssertionError(f"Real dir file not propagated to {name}")
            if target_dir_file.is_symlink():
                raise AssertionError(f"Real dir file is symlink in {name}")
            content = target_dir_file.read_bytes()
            if content != TEST_CONTENT:
                raise AssertionError(f"Real dir file content mismatch in {name}")
            print(f"  OK: {REAL_DIR}/{DIR_FILE} propagated to {name}")

        results["real_file"] = True
        results["real_dir"] = True

        # Check symlinks are NOT propagated to other producers
        # Note: symlinks exist in source_dir (VM created them), but should NOT propagate
        print("\n[Symlinks] Checking symlinks not propagated...")
        time.sleep(5.0)

        # Check symlink file not propagated to other producers
        for name, path in other_producers.items():
            check_path = path / SYMLINK_FILE
            if check_path.exists():
                if check_path.is_symlink():
                    raise AssertionError(
                        f"{SYMLINK_FILE} propagated as symlink to {name} (should be rejected)"
                    )
                else:
                    raise AssertionError(
                        f"{SYMLINK_FILE} propagated as regular file to {name} (unexpected)"
                    )

        results["symlink_file_rejected"] = True
        print(f"  OK: {SYMLINK_FILE} not propagated to other producers")

        # Check symlink dir not propagated to other producers
        for name, path in other_producers.items():
            check_path = path / SYMLINK_DIR
            if check_path.exists():
                if check_path.is_symlink():
                    raise AssertionError(
                        f"{SYMLINK_DIR} propagated as symlink to {name} (should be rejected)"
                    )
                else:
                    raise AssertionError(
                        f"{SYMLINK_DIR} propagated as dir to {name} (unexpected)"
                    )

        results["symlink_dir_rejected"] = True
        print(f"  OK: {SYMLINK_DIR} not propagated to other producers")

        # Summary
        print("\n" + "=" * 50)
        print("SYMLINK REJECTION RESULTS")
        print("=" * 50)
        print(f"  Real file synced: {'PASS' if results['real_file'] else 'FAIL'}")
        print(f"  Real dir synced: {'PASS' if results['real_dir'] else 'FAIL'}")
        print(
            f"  Symlink to file rejected: {'PASS' if results['symlink_file_rejected'] else 'FAIL'}"
        )
        print(
            f"  Symlink to dir rejected: {'PASS' if results['symlink_dir_rejected'] else 'FAIL'}"
        )

        all_passed = all(results.values())
        if all_passed:
            print("\nAll symlink tests passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"Symlink test failed: {failed}")
