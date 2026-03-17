# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""File modify sync test scenario.

Tests:
- F-002: Modified files sync to other producers
- F-008: Empty files (0-byte) sync without ClamAV scan

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s basic.modify --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s basic.modify --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_FILE = TEST_FILE_PREFIX + "modify_test.txt"
EMPTY_FILE = TEST_FILE_PREFIX + "modify_empty.txt"
INITIAL_CONTENT = b"Initial content before modification"
MODIFIED_CONTENT = b"Modified content after update - longer to detect change"


class ModifyScenario:
    """Test file modification sync and empty file handling."""

    def write(self, ctx: TestContext) -> None:
        """Create file, modify it, and create empty file."""
        # F-002: Create and modify file
        print("[F-002] Creating initial file...")
        ctx.create_test_file(TEST_FILE, content=INITIAL_CONTENT)

        time.sleep(1.0)

        print("[F-002] Modifying file...")
        ctx.create_test_file(TEST_FILE, content=MODIFIED_CONTENT)

        # F-008: Create empty file (should bypass scan)
        print("\n[F-008] Creating empty file (0-byte)...")
        ctx.create_test_file(EMPTY_FILE, content=b"")

        print("Modify test: file modified, empty file created")

    def verify(self, ctx: TestContext) -> None:
        """Verify modification and empty file propagated (run on host)."""

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

        # === F-002: Wait for modified file in source ===
        print(f"\n[F-002] Waiting for {TEST_FILE}...")
        source_name = None
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / TEST_FILE).exists():
                    source_dir = path
                    source_name = name
                    print(f"  Found in producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {TEST_FILE}")
            time.sleep(0.1)

        # Wait for modified content to appear in source
        source_file = source_dir / TEST_FILE
        print("  Waiting for modified content...")
        while True:
            content = source_file.read_bytes()
            if content == MODIFIED_CONTENT:
                break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError("Timeout waiting for modified content in source")
            time.sleep(0.1)
        print("  OK: Modified content in source")

        # === F-008: Wait for empty file in source ===
        print(f"\n[F-008] Waiting for empty file {EMPTY_FILE}...")
        empty_file = source_dir / EMPTY_FILE

        while not empty_file.exists():
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {EMPTY_FILE}")
            time.sleep(0.1)

        content = empty_file.read_bytes()
        if len(content) == 0:
            print("  OK: Empty file in source")
        else:
            raise AssertionError(f"Empty file has content: {len(content)} bytes")

        # === Verify propagation to other producers ===
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "F-002: No other producers to verify propagation (need at least 2)"
            )

        print(
            f"\n[F-002] Checking propagation to {len(other_producers)} other producer(s)..."
        )

        for name, path in other_producers.items():
            # Check modified file
            target_file = path / TEST_FILE
            timeout = time.monotonic() + 30.0

            while time.monotonic() < timeout:
                if target_file.exists():
                    content = target_file.read_bytes()
                    if content == MODIFIED_CONTENT:
                        break
                time.sleep(0.1)
            else:
                raise AssertionError(
                    f"Modified file not propagated to {name} (timeout)"
                )

            print(f"  OK: {name} has modified content")

            # Check empty file
            target_empty = path / EMPTY_FILE
            timeout = time.monotonic() + 30.0

            while not target_empty.exists() and time.monotonic() < timeout:
                time.sleep(0.1)

            if not target_empty.exists():
                raise AssertionError(f"Empty file not propagated to {name} (timeout)")

            if len(target_empty.read_bytes()) != 0:
                raise AssertionError(f"Empty file in {name} has content")

            print(f"  OK: {name} has empty file")

        print("\nModify verification passed (F-002 + F-008)")
