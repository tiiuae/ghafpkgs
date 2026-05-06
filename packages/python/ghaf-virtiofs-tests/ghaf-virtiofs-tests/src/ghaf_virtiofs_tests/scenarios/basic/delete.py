# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""File and directory delete sync test scenario.

Tests:
- F-003: Deleted files are removed from other producers
- F-005: New directories are watched, files within sync
- F-006: Deleted directories are removed from other producers

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s basic.delete --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s basic.delete --verify
"""

import shutil
import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_FILE = TEST_FILE_PREFIX + "delete_test.txt"
TEST_DIR = TEST_FILE_PREFIX + "delete_test_dir"
DIR_FILE_1 = TEST_FILE_PREFIX + "file1.txt"
DIR_FILE_2 = TEST_FILE_PREFIX + "file2.txt"
TEST_CONTENT = b"Content to be deleted"


class DeleteScenario:
    """Test file and directory deletion sync."""

    def write(self, ctx: TestContext) -> None:
        """Create file and directory, then delete both."""
        # Test 1: File create and delete
        print("Creating file...")
        ctx.create_test_file(TEST_FILE, content=TEST_CONTENT)

        # Test 2: Directory create with files
        print(f"Creating directory {TEST_DIR}/ with files...")
        dir_path = ctx.path / TEST_DIR
        dir_path.mkdir(parents=True, exist_ok=True)
        (dir_path / DIR_FILE_1).write_bytes(TEST_CONTENT)
        (dir_path / DIR_FILE_2).write_bytes(TEST_CONTENT)

        # Wait for gate to sync files to host before deleting
        print("Waiting for sync before delete...")
        time.sleep(5.0)

        # Test 3: Delete file
        print("Deleting file...")
        file_path = ctx.path / TEST_FILE
        file_path.unlink()

        # Wait for gate to process deletion
        time.sleep(2.0)

        # Test 4: Delete directory tree
        print(f"Deleting directory {TEST_DIR}/...")
        shutil.rmtree(dir_path)

        print("Delete test: file and directory created and deleted")

    def verify(self, ctx: TestContext) -> None:
        """Verify deletions propagated to other producers (run on host)."""

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

        # === Step 1: Wait for TEST_FILE to appear in source ===
        print(f"\n[F-003] Waiting for {TEST_FILE} to appear...")
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

        # === Step 2: Wait for TEST_DIR with files in source ===
        print(f"\n[F-005] Waiting for {TEST_DIR}/ with files...")
        source_test_dir = source_dir / TEST_DIR

        while True:
            if source_test_dir.exists():
                file1 = source_test_dir / DIR_FILE_1
                file2 = source_test_dir / DIR_FILE_2
                if file1.exists() and file2.exists():
                    print("  Directory with files appeared in source")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {TEST_DIR}/")
            time.sleep(0.1)

        # === Check other producers exist ===
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "F-003: No other producers to verify propagation (need at least 2)"
            )

        # === Step 3: Wait for file and dir to appear in other producers ===
        print(
            f"\n[F-005] Checking propagation to {len(other_producers)} other producer(s)..."
        )

        for name, path in other_producers.items():
            # Wait for file
            target_file = path / TEST_FILE
            timeout = time.monotonic() + 30.0
            while not target_file.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_file.exists():
                raise AssertionError(f"File not propagated to {name} (timeout)")
            print(f"  OK: {name} has file")

            # Wait for directory with files
            target_dir = path / TEST_DIR
            timeout = time.monotonic() + 30.0
            while time.monotonic() < timeout:
                if target_dir.exists():
                    f1 = target_dir / DIR_FILE_1
                    f2 = target_dir / DIR_FILE_2
                    if f1.exists() and f2.exists():
                        break
                time.sleep(0.1)
            else:
                raise AssertionError(f"Directory not propagated to {name} (timeout)")
            print(f"  OK: {name} has directory with files")

        # === Step 4: Wait for file deletion in source ===
        print("\n[F-003] Waiting for file deletion in source...")
        source_file = source_dir / TEST_FILE
        while source_file.exists():
            if time.monotonic() - start_time > 120.0:
                raise TimeoutError("Timeout waiting for file deletion in source")
            time.sleep(0.1)
        print("  File deleted from source")

        # === Step 5: Wait for directory deletion in source ===
        print("\n[F-006] Waiting for directory deletion in source...")
        while source_test_dir.exists():
            if time.monotonic() - start_time > 120.0:
                raise TimeoutError("Timeout waiting for directory deletion in source")
            time.sleep(0.1)
        print("  Directory deleted from source")

        # === Step 6: Verify deletions propagated to other producers ===
        print("\n[F-003/F-006] Verifying deletions in other producers...")

        for name, path in other_producers.items():
            # File should be deleted
            target_file = path / TEST_FILE
            timeout = time.monotonic() + 30.0
            while target_file.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if target_file.exists():
                raise AssertionError(f"File not deleted from {name} (timeout)")
            print(f"  OK: {name} file deleted")

            # Directory should be deleted
            target_dir = path / TEST_DIR
            timeout = time.monotonic() + 30.0
            while target_dir.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if target_dir.exists():
                raise AssertionError(f"Directory not deleted from {name} (timeout)")
            print(f"  OK: {name} directory deleted")

        print("\nDelete verification passed (F-003 + F-005 + F-006)")
