# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""File rename sync test scenario.

Tests:
- F-004: File Rename Sync - renamed files sync correctly (no rescan needed)

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s basic.rename --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s basic.rename --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

OLD_NAME = TEST_FILE_PREFIX + "rename_before.txt"
NEW_NAME = TEST_FILE_PREFIX + "rename_after.txt"
TEST_CONTENT = b"Content that will be renamed"


class RenameScenario:
    """Test file rename sync (F-004)."""

    def write(self, ctx: TestContext) -> None:
        """Create file, then rename it."""
        # F-004: File Rename Sync
        print(f"[F-004] Creating {OLD_NAME}...")
        ctx.create_test_file(OLD_NAME, content=TEST_CONTENT)

        # Wait for gate to sync file to host before renaming
        print("Waiting for sync before rename...")
        time.sleep(5.0)

        print(f"Renaming {OLD_NAME} -> {NEW_NAME}...")
        old_path = ctx.path / OLD_NAME
        new_path = ctx.path / NEW_NAME
        old_path.rename(new_path)

        print("Rename test: file created and renamed")

    def verify(self, ctx: TestContext) -> None:
        """Verify rename propagated to other producers (run on host)."""

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

        # Step 1: Wait for OLD_NAME to appear in source
        print(f"\n[F-004] Waiting for {OLD_NAME} to appear...")
        source_name = None
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / OLD_NAME).exists():
                    source_dir = path
                    source_name = name
                    print(f"  Found in producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {OLD_NAME}")
            time.sleep(0.1)

        # Check other producers exist
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "F-004: No other producers to verify propagation (need at least 2)"
            )

        # Step 2: Wait for OLD_NAME to propagate to other producers
        print(
            f"\n[F-004] Checking propagation to {len(other_producers)} other producer(s)..."
        )

        for name, path in other_producers.items():
            target_old = path / OLD_NAME
            timeout = time.monotonic() + 30.0
            while not target_old.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_old.exists():
                raise AssertionError(f"File not propagated to {name} (timeout)")
            print(f"  OK: {name} has {OLD_NAME}")

        # Step 3: Wait for rename in source (old gone, new exists)
        print("\n[F-004] Waiting for rename in source...")
        old_file = source_dir / OLD_NAME
        new_file = source_dir / NEW_NAME

        while old_file.exists() or not new_file.exists():
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError("Timeout waiting for rename in source")
            time.sleep(0.1)

        print("  Renamed in source")

        # Verify content preserved
        content = new_file.read_bytes()
        assert content == TEST_CONTENT, "Content changed after rename"

        # Step 4: Verify rename propagated to other producers
        print("\n[F-004] Verifying rename in other producers...")

        for name, path in other_producers.items():
            target_old = path / OLD_NAME
            target_new = path / NEW_NAME
            timeout = time.monotonic() + 30.0

            while time.monotonic() < timeout:
                if not target_old.exists() and target_new.exists():
                    break
                time.sleep(0.1)
            else:
                old_exists = target_old.exists()
                new_exists = target_new.exists()
                raise AssertionError(
                    f"Rename not propagated to {name}: old={old_exists}, new={new_exists}"
                )

            # Verify content
            content = target_new.read_bytes()
            assert content == TEST_CONTENT, f"Content mismatch in {name}"
            print(f"  OK: {name} renamed")

        print("\nRename verification passed (F-004)")
