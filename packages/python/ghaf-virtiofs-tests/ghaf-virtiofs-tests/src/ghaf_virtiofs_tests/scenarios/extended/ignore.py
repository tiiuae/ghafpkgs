# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Ignore patterns test (extended).

Tests:
- F-010: Ignore file patterns (.crdownload, .part, .tmp, ~$)
- F-011: Ignore path patterns (.Trash-) including nested paths

Files matching ignore patterns should NOT be synced.
Control file (no ignore pattern) should sync normally.

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.ignore --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.ignore --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_CONTENT = b"Ignore pattern test content"

# Control file - should sync
CONTROL_FILE = TEST_FILE_PREFIX + "ignore_control.txt"

# F-010: File patterns that should be ignored
IGNORED_FILE_PATTERNS = [
    TEST_FILE_PREFIX + "download.crdownload",
    TEST_FILE_PREFIX + "partial.part",
    TEST_FILE_PREFIX + "tempfile.tmp",
    TEST_FILE_PREFIX + "~$document.docx",
]

# F-011: Path patterns that should be ignored
IGNORED_PATH_PATTERNS = [
    TEST_FILE_PREFIX + ".Trash-1000/file.txt",
    TEST_FILE_PREFIX + ".local/share/.Trash-1234/deleted.txt",
]


class IgnoreScenario:
    """Test ignore patterns."""

    def write(self, ctx: TestContext) -> None:
        """Create control file and files matching ignore patterns."""

        # Create control file (should sync)
        print(f"Creating control file: {CONTROL_FILE}")
        ctx.create_test_file(CONTROL_FILE, content=TEST_CONTENT)

        # Create files matching ignore file patterns
        print("\nCreating files with ignored extensions...")
        for filename in IGNORED_FILE_PATTERNS:
            try:
                ctx.create_test_file(filename, content=TEST_CONTENT)
                print(f"  Created: {filename}")
            except Exception as e:
                print(f"  Failed: {filename} - {e}")

        # Create files in ignored path patterns
        print("\nCreating files in ignored paths...")
        for filepath in IGNORED_PATH_PATTERNS:
            try:
                file_path = ctx.path / filepath
                file_path.parent.mkdir(parents=True, exist_ok=True)
                file_path.write_bytes(TEST_CONTENT)
                print(f"  Created: {filepath}")
            except Exception as e:
                print(f"  Failed: {filepath} - {e}")

        print("\nIgnore test: files created")
        print("Expected: control file syncs, ignored patterns do NOT sync")

    def verify(self, ctx: TestContext) -> None:
        """Verify control syncs but ignored patterns don't (run on host)."""

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

        # Wait for control file to detect source producer
        print(f"\nWaiting for control file: {CONTROL_FILE}")
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / CONTROL_FILE).exists():
                    source_dir = path
                    print(f"  Found source producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {CONTROL_FILE}")
            time.sleep(0.1)

        results = {
            "control_synced": False,
            "file_patterns_ignored": [],
            "path_patterns_ignored": [],
        }

        # Find other producers (not source) to check propagation
        other_producers = {n: p for n, p in producer_dirs.items() if p != source_dir}

        if not other_producers:
            raise RuntimeError(
                "Ignore test: No other producers to verify propagation (need at least 2)"
            )

        # Verify control file propagated to other producers
        print("\n[Control] Checking control file propagated...")
        for name, path in other_producers.items():
            target_control = path / CONTROL_FILE
            timeout = time.monotonic() + 30.0
            while not target_control.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_control.exists():
                raise AssertionError(f"Control file not propagated to {name}")
            content = target_control.read_bytes()
            if content != TEST_CONTENT:
                raise AssertionError(f"Control file content mismatch in {name}")
            print(f"  OK: {CONTROL_FILE} propagated to {name}")
        results["control_synced"] = True

        # Wait a bit to ensure ignored files had time to sync (if they were going to)
        time.sleep(5.0)

        # F-010: Check file patterns are NOT propagated to other producers
        # Note: files exist in source_dir (VM created them), but should NOT propagate
        print("\n[F-010] Checking file patterns not propagated...")
        for filename in IGNORED_FILE_PATTERNS:
            for name, path in other_producers.items():
                if (path / filename).exists():
                    raise AssertionError(
                        f"{filename} propagated to {name} (should be ignored)"
                    )
            results["file_patterns_ignored"].append(filename)
            print(f"  OK: {filename} not propagated")

        # F-011: Check path patterns are NOT propagated to other producers
        print("\n[F-011] Checking path patterns not propagated...")
        for filepath in IGNORED_PATH_PATTERNS:
            for name, path in other_producers.items():
                if (path / filepath).exists():
                    raise AssertionError(
                        f"{filepath} propagated to {name} (should be ignored)"
                    )
            results["path_patterns_ignored"].append(filepath)
            print(f"  OK: {filepath} not propagated")

        # Summary
        print("\n" + "=" * 50)
        print("IGNORE PATTERN RESULTS")
        print("=" * 50)
        print(
            f"  Control file synced: {'PASS' if results['control_synced'] else 'FAIL'}"
        )
        print(
            f"  File patterns ignored: {len(results['file_patterns_ignored'])}/{len(IGNORED_FILE_PATTERNS)}"
        )
        print(
            f"  Path patterns ignored: {len(results['path_patterns_ignored'])}/{len(IGNORED_PATH_PATTERNS)}"
        )

        total_ignored = len(IGNORED_FILE_PATTERNS) + len(IGNORED_PATH_PATTERNS)
        actual_ignored = len(results["file_patterns_ignored"]) + len(
            results["path_patterns_ignored"]
        )

        if results["control_synced"] and actual_ignored == total_ignored:
            print("\nAll ignore pattern tests passed")
        else:
            failed = []
            if not results["control_synced"]:
                failed.append("control_not_synced")
            if actual_ignored < total_ignored:
                failed.append(f"ignored_only_{actual_ignored}_of_{total_ignored}")
            raise AssertionError(f"Ignore pattern test failed: {failed}")
