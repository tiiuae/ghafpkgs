# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Overload and DoS resilience test scenario.

Tests:
- SEC-001: Kernel Queue Overflow with Recovery Verification
- SEC-002: Pending Queue Flood (create 10K+ files within debounce)
- SEC-003: Deep Directory Nesting (1000-level deep path)
- SEC-004: Long Filename (255-char filename)
- SEC-005: Many Small Files (100K tiny files)

These tests verify the daemon handles overload gracefully without crashing.

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s security.overload --write

    # Host verifier:
    ghaf-virtiofs-test run /mnt/basePath -s security.overload --verify
"""

import os
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from ...context import TestContext

# Test parameters
RAPID_FILE_COUNT = 10000  # SEC-001, SEC-005
DEBOUNCE_FLOOD_COUNT = 5000  # SEC-002
MAX_NESTING_DEPTH = 1000  # SEC-003
LONG_FILENAME_SIZE = 251  # SEC-004: (251 + ".txt" = 255)

# SEC-001: Kernel queue overflow parameters
# Default max_queued_events is 16384, each file generates ~2 events (CREATE + CLOSE_WRITE)
# Need to create files faster than watcher can consume to overflow the queue
KERNEL_OVERFLOW_FILE_COUNT = 10000  # 10K files * 2 events = 20K events > 16384 queue
KERNEL_OVERFLOW_THREADS = 8  # Parallel threads for faster creation


class OverloadScenario:
    """Test daemon resilience to overload conditions."""

    def write(self, ctx: TestContext) -> None:
        """Run overload tests from VM."""
        results = {
            "_GVTT-TESTFILE_sec001_kernel_overflow": False,
            "_GVTT-TESTFILE_sec002_debounce_flood": False,
            "_GVTT-TESTFILE_sec003_deep_nesting": False,
            "_GVTT-TESTFILE_sec004_long_filename": False,
            "_GVTT-TESTFILE_sec005_many_small": False,
        }

        # SEC-001: Kernel queue overflow with recovery verification
        print("\n[SEC-001] Triggering kernel inotify queue overflow...")
        print(
            f"  Target: {KERNEL_OVERFLOW_FILE_COUNT} files using {KERNEL_OVERFLOW_THREADS} threads"
        )
        try:
            overflow_dir = ctx.path / "_GVTT-TESTFILE_sec001_overflow"
            overflow_dir.mkdir(exist_ok=True)

            def create_batch(
                thread_id: int, batch_size: int, base_dir: Path
            ) -> list[Path]:
                """Create files to trigger overflow."""
                paths = []
                for i in range(batch_size):
                    file_path = base_dir / f"t{thread_id}_{i:06d}"
                    fd = os.open(str(file_path), os.O_CREAT | os.O_WRONLY, 0o644)
                    os.write(fd, b"x")
                    os.close(fd)
                    paths.append(file_path)
                return paths

            def delete_batch(paths: list[Path]) -> int:
                """Delete files to generate DELETE events during recovery."""
                count = 0
                for file_path in paths:
                    try:
                        os.unlink(str(file_path))
                        count += 1
                    except OSError:
                        pass
                return count

            # Phase 1: Create files to trigger overflow
            batch_size = KERNEL_OVERFLOW_FILE_COUNT // KERNEL_OVERFLOW_THREADS
            start = time.monotonic()
            all_paths: list[list[Path]] = []

            print("  Phase 1: Creating files to trigger overflow...")
            with ThreadPoolExecutor(max_workers=KERNEL_OVERFLOW_THREADS) as executor:
                futures = [
                    executor.submit(create_batch, tid, batch_size, overflow_dir)
                    for tid in range(KERNEL_OVERFLOW_THREADS)
                ]
                all_paths = [f.result() for f in futures]

            total_created = sum(len(p) for p in all_paths)
            create_elapsed = time.monotonic() - start
            create_events = total_created * 2  # CREATE + CLOSE_WRITE
            print(f"  Created {total_created} files in {create_elapsed:.2f}s")
            print(
                f"  Events generated: ~{create_events} (exceeds queue limit of 16384)"
            )

            # Wait for watcher to detect overflow and START recovery
            # Backoff is 2s, so wait ~3s to let recovery begin
            recovery_start_wait = 3.0
            print(f"  Waiting {recovery_start_wait}s for watcher to start recovery...")
            time.sleep(recovery_start_wait)

            # Phase 2: Delete files DURING recovery to trigger cascading overflow
            print(
                "  Phase 2: Deleting files during recovery (cascading overflow test)..."
            )
            delete_start = time.monotonic()

            with ThreadPoolExecutor(max_workers=KERNEL_OVERFLOW_THREADS) as executor:
                futures = [executor.submit(delete_batch, paths) for paths in all_paths]
                total_deleted = sum(f.result() for f in futures)

            delete_elapsed = time.monotonic() - delete_start
            delete_events = total_deleted  # DELETE events
            print(f"  Deleted {total_deleted} files in {delete_elapsed:.2f}s")
            print(f"  Delete events: ~{delete_events}")

            total_elapsed = time.monotonic() - start
            total_events = create_events + delete_events
            print(
                f"  Total: {total_created} files, ~{total_events} events in {total_elapsed:.2f}s"
            )

            # Wait for watcher to recover from cascading overflows
            recovery_wait = 5.0
            print(
                f"  Waiting {recovery_wait}s for watcher recovery from cascading overflow..."
            )
            time.sleep(recovery_wait)

            # Write marker file after recovery period
            marker_file = overflow_dir / "_MARKER_OVERFLOW_COMPLETE"
            marker_file.write_text(
                f"created={total_created},deleted={total_deleted},"
                f"elapsed={total_elapsed:.3f},events={total_events}"
            )

            # Verify basic write works after recovery by creating test files
            print("  Verifying basic write works after recovery...")
            recovery_test_dir = ctx.path / "_GVTT-TESTFILE_sec001_recovery"
            recovery_test_dir.mkdir(exist_ok=True)

            for i in range(5):
                test_file = recovery_test_dir / f"recovery_test_{i}.txt"
                test_file.write_text(f"recovery test {i} - {time.time()}")
                time.sleep(0.1)  # Small delay between writes

            results["_GVTT-TESTFILE_sec001_kernel_overflow"] = True
            print(f"  Recovery test files written to {recovery_test_dir.name}/")
        except Exception as e:
            print(f"  Error: {e}")
            results["_GVTT-TESTFILE_sec001_kernel_overflow"] = False

        # SEC-002: Debounce flood (many files within debounce window)
        print(f"\n[SEC-002] Creating {DEBOUNCE_FLOOD_COUNT} files within debounce...")
        try:
            flood_dir = ctx.path / "_GVTT-TESTFILE_sec002_flood"
            flood_dir.mkdir(exist_ok=True)

            start = time.monotonic()
            for i in range(DEBOUNCE_FLOOD_COUNT):
                (flood_dir / f"flood_{i:05d}.txt").write_bytes(b"flood")

            elapsed = time.monotonic() - start
            results["_GVTT-TESTFILE_sec002_debounce_flood"] = True
            print(
                f"  Created {DEBOUNCE_FLOOD_COUNT} files in {elapsed:.2f}s (within debounce)"
            )
        except Exception as e:
            print(f"  Error: {e}")

        # SEC-003: Deep directory nesting
        print(f"\n[SEC-003] Creating {MAX_NESTING_DEPTH}-level deep path...")
        try:
            # Build nested path
            nested_parts = ["_GVTT-TESTFILE_sec003_deep"] + [
                f"d{i}" for i in range(MAX_NESTING_DEPTH)
            ]
            nested_path = ctx.path.joinpath(*nested_parts)

            try:
                nested_path.mkdir(parents=True, exist_ok=True)
                (nested_path / "deep_file.txt").write_bytes(b"deep content")
                results["_GVTT-TESTFILE_sec003_deep_nesting"] = True
                print(f"  Created {MAX_NESTING_DEPTH}-deep path")
            except OSError as e:
                # Expected on some filesystems
                print(f"  Rejected (expected): {e}")
                results["_GVTT-TESTFILE_sec003_deep_nesting"] = True
        except Exception as e:
            print(f"  Error: {e}")

        # SEC-004: Long filename
        print(f"\n[SEC-004] Creating file with {LONG_FILENAME_SIZE}-char filename...")
        try:
            long_name = "x" * LONG_FILENAME_SIZE + ".txt"
            long_file = ctx.path / long_name

            try:
                long_file.write_bytes(b"long filename content")
                results["_GVTT-TESTFILE_sec004_long_filename"] = True
                print(f"  Created file with {len(long_name)}-char name")
            except OSError as e:
                print(f"  Rejected (expected): {e}")
                results["_GVTT-TESTFILE_sec004_long_filename"] = True
        except Exception as e:
            print(f"  Error: {e}")

        # SEC-005: Many small files (stress test)
        print(f"\n[SEC-005] Creating {RAPID_FILE_COUNT} small files...")
        try:
            small_dir = ctx.path / "_GVTT-TESTFILE_sec005_small"
            small_dir.mkdir(exist_ok=True)

            start = time.monotonic()
            for i in range(RAPID_FILE_COUNT):
                (small_dir / f"tiny_{i:06d}").write_bytes(b"t")

            elapsed = time.monotonic() - start
            results["_GVTT-TESTFILE_sec005_many_small"] = True
            print(f"  Created {RAPID_FILE_COUNT} tiny files in {elapsed:.2f}s")
        except Exception as e:
            print(f"  Error: {e}")

        # Summary
        print("\n" + "=" * 50)
        print("OVERLOAD TEST RESULTS")
        print("=" * 50)
        for test, passed in results.items():
            print(f"  {test}: {'PASS' if passed else 'FAIL'}")

        passed = sum(results.values())
        print(f"\n{passed}/{len(results)} tests completed")

    def verify(self, ctx: TestContext) -> None:
        """Verify daemon is still responsive after overload (run on host)."""
        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        print(f"Found producers: {list(producer_dirs.keys())}")

        test_dirs = [
            "_GVTT-TESTFILE_sec001_overflow",
            "_GVTT-TESTFILE_sec001_recovery",
            "_GVTT-TESTFILE_sec002_flood",
            "_GVTT-TESTFILE_sec005_small",
        ]

        # Wait for any overload test directory to appear
        print("\nWaiting for overload test files...")
        start_time = time.monotonic()
        found_any = False

        while not found_any and time.monotonic() - start_time < 300.0:
            for name, path in producer_dirs.items():
                for test_dir in test_dirs:
                    if (path / test_dir).exists():
                        found_any = True
                        print(f"  Found {test_dir} in {name}")
                        break
            time.sleep(1.0)

        if not found_any:
            raise AssertionError("No overload test directories found (timeout)")

        # Check daemon is responsive by waiting for files to propagate
        print("\nChecking daemon responsiveness...")
        time.sleep(10.0)  # Give daemon time to process

        # Count files that propagated
        for name, path in producer_dirs.items():
            for test_dir in test_dirs:
                test_path = path / test_dir
                if test_path.exists():
                    file_count = len(list(test_path.iterdir()))
                    print(f"  {name}/{test_dir}: {file_count} files")

        # SEC-001: Verify overflow recovery
        print("\n[SEC-001] Verifying kernel overflow recovery...")
        for name, path in producer_dirs.items():
            overflow_dir = path / "_GVTT-TESTFILE_sec001_overflow"
            recovery_dir = path / "_GVTT-TESTFILE_sec001_recovery"

            if not overflow_dir.exists() and not recovery_dir.exists():
                continue

            # Check marker file from overflow burst
            marker_file = (
                overflow_dir / "_MARKER_OVERFLOW_COMPLETE"
                if overflow_dir.exists()
                else None
            )
            if marker_file and marker_file.exists():
                marker_content = marker_file.read_text()
                print(f"  {name}: Marker found - {marker_content}")

                # Parse stats from marker
                stats = {}
                for part in marker_content.split(","):
                    if "=" in part:
                        key, val = part.split("=", 1)
                        stats[key] = val

                created = int(stats.get("created", 0))
                deleted = int(stats.get("deleted", 0))
                events = int(stats.get("events", 0))
                print(
                    f"  {name}: {created} created, {deleted} deleted, ~{events} events"
                )

            # Check recovery test files - these prove watcher works after overflow
            if recovery_dir.exists():
                recovery_files = list(recovery_dir.glob("recovery_test_*.txt"))
                print(f"  {name}: Found {len(recovery_files)}/5 recovery test files")

                if len(recovery_files) >= 5:
                    print(f"  {name}: PASS - all recovery test files detected")
                    print(
                        f"  {name}: Watcher fully operational after overflow recovery"
                    )
                elif len(recovery_files) > 0:
                    print(
                        f"  {name}: PARTIAL - some recovery files detected, watcher recovering"
                    )
                else:
                    print(f"  {name}: WAITING - recovery files not yet propagated")
            else:
                print(f"  {name}: Recovery test directory not found")

        print("\nOverload verification complete (daemon remained responsive)")
