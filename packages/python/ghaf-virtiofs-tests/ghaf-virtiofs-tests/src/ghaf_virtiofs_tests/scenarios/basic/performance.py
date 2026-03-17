# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Performance benchmark scenario.

Tests:
- P-001: 100KB file propagation throughput
- P-002: 1MB file propagation throughput
- P-003: 10MB file propagation throughput
- P-004: 100MB file propagation throughput
- P-005: 1000MB file propagation throughput
- F-009: Debounce consolidation (rapid writes consolidated)

Directory structure (basePath):
    share/
        writer-vm/      <- files appear here first (gate input)
        other-vm/       <- gate syncs here (propagation target)

Usage:
    # Writer VM (writes to its producer share):
    ghaf-virtiofs-test run /mnt/share -s basic.performance --write

    # Host verifier (monitors basePath, measures gate timing):
    ghaf-virtiofs-test run /mnt/basePath -s basic.performance --verify
"""

import secrets
import time
from pathlib import Path

from ...context import TEST_FILE_PREFIX, TestContext


class PerformanceScenario:
    """Benchmark gate propagation performance."""

    FILE_SIZES = [
        ("100KB", 100 * 1024),  # P-001
        ("1MB", 1024 * 1024),  # P-002
        ("10MB", 10 * 1024 * 1024),  # P-003
        ("100MB", 100 * 1024 * 1024),  # P-004
        ("1000MB", 1000 * 1024 * 1024),  # P-005
    ]
    FILES_PER_SIZE = 1
    FILE_PATTERN = TEST_FILE_PREFIX + "perf_*_*.bin"
    EXPECTED_FILES = len(FILE_SIZES) * FILES_PER_SIZE  # 5

    # F-009: Debounce consolidation test
    DEBOUNCE_FILE = TEST_FILE_PREFIX + "perf_debounce.txt"
    DEBOUNCE_WRITES = 10
    DEBOUNCE_INTERVAL_MS = 10  # 10ms between writes = 100ms total

    def write(self, ctx: TestContext) -> None:
        """Create test files for performance measurement."""
        ctx.metrics.set_metadata("test_type", "performance")
        ctx.metrics.set_metadata("files_per_size", self.FILES_PER_SIZE)
        ctx.metrics.set_metadata("file_sizes", [s[0] for s in self.FILE_SIZES])

        # F-009: Debounce consolidation test - rapid writes to same file
        print(f"[F-009] Debounce test: {self.DEBOUNCE_WRITES} writes in 100ms...")
        debounce_file = ctx.path / self.DEBOUNCE_FILE

        for i in range(self.DEBOUNCE_WRITES):
            content = f"Write #{i + 1} of {self.DEBOUNCE_WRITES}\n".encode()
            debounce_file.write_bytes(content)
            time.sleep(self.DEBOUNCE_INTERVAL_MS / 1000)

        # Final write with marker content
        final_content = f"FINAL: Write #{self.DEBOUNCE_WRITES} complete\n".encode()
        debounce_file.write_bytes(final_content)
        print("  Rapid writes complete")

        total_bytes = 0

        for size_name, size_bytes in self.FILE_SIZES:
            print(f"Creating {self.FILES_PER_SIZE} x {size_name} files...")

            for i in range(self.FILES_PER_SIZE):
                filename = f"{TEST_FILE_PREFIX}perf_{size_name}_{i}.bin"
                content = secrets.token_bytes(size_bytes)

                start = time.monotonic()
                ctx.create_test_file(filename, content=content)
                elapsed_ms = (time.monotonic() - start) * 1000

                ctx.metrics.record(f"write_{size_name}_ms", elapsed_ms)
                total_bytes += size_bytes

        ctx.metrics.set_metadata("total_bytes", total_bytes)
        print(
            f"Created {self.EXPECTED_FILES} files ({total_bytes / (1024 * 1024):.2f} MB)"
        )

    def verify(self, ctx: TestContext) -> None:
        """Monitor basePath for gate propagation timing (run on host)."""
        ctx.metrics.set_metadata("test_type", "performance")

        # Build size lookup for throughput calculation
        size_lookup = dict(self.FILE_SIZES)

        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Discover all producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        if not producer_dirs:
            raise RuntimeError(f"No producer directories in {share_dir}")

        print(f"Found producers: {list(producer_dirs.keys())}")
        start_time = time.monotonic()

        # Detect source producer (first dir where files appear)
        source_name = None
        source_dir = None

        while source_name is None:
            for name, path in producer_dirs.items():
                if (path / self.DEBOUNCE_FILE).exists() or list(
                    path.glob(self.FILE_PATTERN)
                ):
                    source_name = name
                    source_dir = path
                    break
            if time.monotonic() - start_time > 300.0:
                raise TimeoutError("Timeout detecting source producer")
            time.sleep(0.01)

        print(f"Source producer: {source_name}")

        # === F-009: Verify debounce consolidation ===
        # Test: when file first appears in target, it should have final content
        # If debounce failed (multiple syncs), we might see intermediate content
        print("\n[F-009] Checking debounce consolidation...")
        final_marker = f"FINAL: Write #{self.DEBOUNCE_WRITES} complete\n".encode()

        # Find target producers (all except source)
        target_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not target_producers:
            raise RuntimeError(
                "F-009: No target producers found (need at least 2 producers)"
            )

        # Wait for debounce file to appear in ANY target producer
        debounce_target_path = None
        debounce_target_name = None
        timeout = time.monotonic() + 60.0

        while time.monotonic() < timeout:
            for name, path in target_producers.items():
                check_path = path / self.DEBOUNCE_FILE
                if check_path.exists():
                    debounce_target_path = check_path
                    debounce_target_name = name
                    break
            if debounce_target_path:
                break
            time.sleep(0.05)  # Poll quickly to catch first appearance
        else:
            raise TimeoutError("F-009: Timeout waiting for debounce file in target")

        # Immediately check content - should be final, not intermediate
        content = debounce_target_path.read_bytes()
        if content == final_marker:
            print(f"  OK: {debounce_target_name} has final content on first sync")
        else:
            raise AssertionError(
                f"F-009: {debounce_target_name} has intermediate content: {content!r}"
            )

        # === Size-based performance tests ===
        print(
            f"\nWaiting for {self.EXPECTED_FILES} files matching {self.FILE_PATTERN}..."
        )

        # Build target locations (other producers)
        targets: dict[str, Path] = {
            n: p for n, p in producer_dirs.items() if n != source_name
        }

        if not targets:
            raise RuntimeError(
                "Performance test: No other producers to verify propagation (need at least 2)"
            )

        print(f"Target producers: {list(targets.keys())}")

        # Wait for all files in source and record when we first detect them
        source_detected: dict[str, float] = {}
        seen_files: set[str] = set()

        print("Recording source file detection times...")
        while len(seen_files) < self.EXPECTED_FILES:
            for file_path in source_dir.glob(self.FILE_PATTERN):
                filename = file_path.name
                if filename not in seen_files:
                    seen_files.add(filename)
                    # Record when we first detected the file
                    source_detected[filename] = time.monotonic()

            if time.monotonic() - start_time > 300.0:
                raise TimeoutError(
                    f"Timeout: only {len(seen_files)}/{self.EXPECTED_FILES} files in source"
                )
            time.sleep(0.01)

        print(f"All {len(seen_files)} files arrived in source")

        # Wait for propagation to all targets
        propagation: dict[str, dict[str, list[float]]] = {t: {} for t in targets}

        for target_name, target_path in targets.items():
            print(f"Monitoring {target_name}...")
            target_seen: set[str] = set()

            while len(target_seen) < self.EXPECTED_FILES:
                for file_path in target_path.glob(self.FILE_PATTERN):
                    filename = file_path.name
                    if filename not in target_seen:
                        target_seen.add(filename)

                        # Detection-based timing: time from source detection to target detection
                        target_detected = time.monotonic()
                        source_time = source_detected[filename]
                        prop_ms = (target_detected - source_time) * 1000

                        # Extract size name from filename (e.g., _GVTT-TESTFILE_perf_100KB_0.bin)
                        size_name = None
                        for label in size_lookup:
                            if label in filename:
                                size_name = label
                                break
                        if size_name is None:
                            continue

                        ctx.metrics.record(
                            f"propagate_{target_name}_{size_name}_ms", prop_ms
                        )

                        # Calculate throughput (MB/s)
                        size_bytes = size_lookup[size_name]
                        if prop_ms > 0:
                            throughput_mbps = (size_bytes / (1024 * 1024)) / (
                                prop_ms / 1000
                            )
                            ctx.metrics.record(
                                f"throughput_{target_name}_{size_name}_mbps",
                                throughput_mbps,
                            )

                        if size_name not in propagation[target_name]:
                            propagation[target_name][size_name] = []
                        propagation[target_name][size_name].append(
                            (prop_ms, size_bytes)
                        )

                if time.monotonic() - start_time > 300.0:
                    raise TimeoutError(
                        f"Timeout: {len(target_seen)}/{self.EXPECTED_FILES} in {target_name}"
                    )
                time.sleep(0.01)

            print(f"  {target_name}: done")

        total_time = time.monotonic() - start_time
        ctx.metrics.record("total_propagation_s", total_time)

        print(f"All propagation complete in {total_time:.2f}s")

        # Print summary
        print("\nPropagation Summary:")
        for target_name, sizes in propagation.items():
            for size_name, entries in sizes.items():
                times = [e[0] for e in entries]
                size_bytes = entries[0][1]
                avg_ms = sum(times) / len(times)
                throughput = (
                    (size_bytes / (1024 * 1024)) / (avg_ms / 1000) if avg_ms > 0 else 0
                )
                print(
                    f"  {target_name}/{size_name}: {avg_ms:.0f}ms ({throughput:.1f} MB/s)"
                )
