# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Scan overhead measurement scenario (host only).

Measures end-to-end propagation time including scan overhead.
Writes files to one producer share and measures time until
they appear in another producer.

Tests:
- P-006: Propagation overhead for 10MB
- P-007: Propagation overhead for 100MB
- P-008: Propagation overhead for 1GB
- P-009: Propagation overhead for 5GB

Usage:
    # Host only (needs basePath with share/ containing multiple producers):
    ghaf-virtiofs-test run /persist/storagevm/channel -s extended.scan_overhead --verify
"""

import secrets
import time
from pathlib import Path

from ...context import TEST_FILE_PREFIX, TestContext

CHUNK_SIZE = 10 * 1024 * 1024  # 10MB chunks for large file creation

FILE_SIZES = [
    ("P-006", "10MB", 10 * 1024 * 1024),
    ("P-007", "100MB", 100 * 1024 * 1024),
    ("P-008", "1GB", 1024 * 1024 * 1024),
    ("P-009", "5GB", 5 * 1024 * 1024 * 1024),
]


def create_random_file(file_path: Path, size_bytes: int) -> None:
    """Create a file with random content using chunked writes."""
    with file_path.open("wb") as f:
        remaining = size_bytes
        while remaining > 0:
            chunk_size = min(CHUNK_SIZE, remaining)
            f.write(secrets.token_bytes(chunk_size))
            remaining -= chunk_size


class ScanOverheadScenario:
    """Measure end-to-end propagation overhead including scan time."""

    def write(self, ctx: TestContext) -> None:
        """Not used - this is a host-only test."""
        print("extended.scan_overhead is host-only (use --verify on host)")

    def verify(self, ctx: TestContext) -> None:
        """Measure propagation time from one producer to another."""
        ctx.metrics.set_metadata("test_type", "scan_overhead")

        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Find producer directories
        producer_dirs = sorted([d for d in share_dir.iterdir() if d.is_dir()])
        if len(producer_dirs) < 2:
            raise RuntimeError("Scan overhead test: Need at least 2 producers")

        source_dir = producer_dirs[0]
        target_dir = producer_dirs[1]
        print(f"Using: {source_dir.name} -> {target_dir.name}")

        results = {}

        for test_id, size_name, size_bytes in FILE_SIZES:
            filename = f"{TEST_FILE_PREFIX}overhead_{size_name}.bin"
            source_file = source_dir / filename
            target_file = target_dir / filename

            print(f"\n[{test_id}] Testing {size_name}...")

            # Clean up any previous test file
            if source_file.exists():
                source_file.unlink()
            if target_file.exists():
                target_file.unlink()

            # Create file and start timing
            start_time = time.monotonic()
            create_random_file(source_file, size_bytes)
            write_done = time.monotonic()

            # Wait for propagation to target
            timeout = 600.0  # 10 minutes for large files
            while not target_file.exists():
                if time.monotonic() - start_time > timeout:
                    print("  TIMEOUT waiting for propagation")
                    break
                time.sleep(0.05)

            if target_file.exists():
                propagation_done = time.monotonic()

                # Verify size matches
                if target_file.stat().st_size == size_bytes:
                    total_ms = (propagation_done - start_time) * 1000
                    write_ms = (write_done - start_time) * 1000
                    overhead_ms = (propagation_done - write_done) * 1000

                    results[size_name] = overhead_ms
                    ctx.metrics.record(f"propagation_{size_name}_ms", total_ms)
                    ctx.metrics.record(f"write_{size_name}_ms", write_ms)
                    ctx.metrics.record(f"overhead_{size_name}_ms", overhead_ms)

                    print(
                        f"  Write: {write_ms:.0f}ms, Overhead: {overhead_ms:.0f}ms, Total: {total_ms:.0f}ms"
                    )
                else:
                    raise AssertionError(f"Scan overhead {size_name}: Size mismatch")

            # Clean up
            if source_file.exists():
                source_file.unlink()
            if target_file.exists():
                target_file.unlink()

        # Summary
        print("\n" + "=" * 50)
        print("SCAN OVERHEAD RESULTS")
        print("=" * 50)
        for size_name, ms in results.items():
            print(f"  {size_name}: {ms:.0f}ms overhead")

        if "10MB" in results and "100MB" in results:
            ratio = results["100MB"] / results["10MB"]
            ctx.metrics.record("overhead_ratio_100MB_vs_10MB", ratio)
            print(f"\n  100MB/10MB overhead ratio: {ratio:.1f}x")

        print("\nScan overhead measurement complete")
