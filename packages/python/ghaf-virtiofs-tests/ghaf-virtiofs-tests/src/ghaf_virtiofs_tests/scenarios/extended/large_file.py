# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Large file test scenario (extended).

Tests:
- P-013: 2GB file propagation
- P-014: 4GB file propagation
- P-015: 8GB file propagation
- P-016: 10GB file propagation
- P-017: 12GB file propagation
- P-018: 16GB file propagation

Measures large file propagation performance.
Files are written sequentially; host reports success or failure per file.

Usage:
    # Writer VM:
    ghaf-virtiofs-test run /mnt/share -s extended.large_file -c /tmp --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.large_file -c /tmp --verify
"""

import secrets
import time

from ...context import TEST_FILE_PREFIX, TestContext

GB = 1024 * 1024 * 1024
CHUNK_SIZE = 64 * 1024 * 1024  # 64MB chunks for large files


class LargeFileScenario:
    """Test large file handling - find ClamAV limits."""

    FILE_SIZES = [
        ("2GB", 2 * GB),
        ("4GB", 4 * GB),
        ("8GB", 8 * GB),
        ("10GB", 10 * GB),
        ("12GB", 12 * GB),
        ("16GB", 16 * GB),
    ]
    FILE_PATTERN = TEST_FILE_PREFIX + "large_*_*.bin"
    EXPECTED_FILES = len(FILE_SIZES)

    def write(self, ctx: TestContext) -> None:
        """Create large test files sequentially."""
        ctx.metrics.set_metadata("test_type", "large_file")
        ctx.metrics.set_metadata("file_sizes", [s[0] for s in self.FILE_SIZES])

        total_bytes = 0

        for size_name, size_bytes in self.FILE_SIZES:
            filename = f"{TEST_FILE_PREFIX}large_{size_name}_0.bin"
            print(f"Creating {size_name} file: {filename}...")

            start = time.monotonic()

            # Write in chunks to avoid memory issues
            file_path = ctx.path / filename
            file_path.parent.mkdir(parents=True, exist_ok=True)

            bytes_written = 0
            with open(file_path, "wb") as f:
                while bytes_written < size_bytes:
                    remaining = size_bytes - bytes_written
                    write_size = min(CHUNK_SIZE, remaining)
                    f.write(secrets.token_bytes(write_size))
                    bytes_written += write_size

            elapsed_s = time.monotonic() - start
            throughput_mbps = (size_bytes / (1024 * 1024)) / elapsed_s

            ctx.metrics.record(f"write_{size_name}_s", elapsed_s)
            ctx.metrics.record(f"write_{size_name}_mbps", throughput_mbps)
            total_bytes += size_bytes

            print(f"  Written in {elapsed_s:.2f}s ({throughput_mbps:.2f} MB/s)")

        ctx.metrics.set_metadata("total_bytes", total_bytes)
        print(f"Created {len(self.FILE_SIZES)} files ({total_bytes / GB:.2f} GB)")

    def verify(self, ctx: TestContext) -> None:
        """Monitor for large file propagation (run on host)."""
        ctx.metrics.set_metadata("test_type", "large_file")

        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Discover producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        if not producer_dirs:
            raise RuntimeError(f"No producer directories in {share_dir}")

        print(f"Found producers: {list(producer_dirs.keys())}")

        # Detect source producer
        source_name = None
        source_dir = None
        start_time = time.monotonic()

        print("Waiting for source producer...")
        while source_name is None:
            for name, path in producer_dirs.items():
                if list(path.glob(self.FILE_PATTERN)):
                    source_name = name
                    source_dir = path
                    break
            if time.monotonic() - start_time > 120.0:  # 2 min timeout for large files
                raise TimeoutError("Timeout detecting source producer")
            time.sleep(0.1)

        print(f"Source producer: {source_name}")

        # Other producers for propagation verification
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "Large file test: No other producers to verify propagation (need at least 2)"
            )

        print(f"Target producers: {list(other_producers.keys())}")

        # Track results per file
        results: dict[str, dict] = {}

        for size_name, size_bytes in self.FILE_SIZES:
            filename = f"{TEST_FILE_PREFIX}large_{size_name}_0.bin"
            results[size_name] = {"size_bytes": size_bytes, "status": "pending"}

            print(f"\nChecking {size_name} ({filename})...")

            # Wait for file in source - use per-file timeout
            source_file = source_dir / filename
            file_start = time.monotonic()
            file_timeout = 120.0  # 2 min timeout per file

            print("  Waiting for file in source...")
            while not source_file.exists():
                if time.monotonic() - file_start > file_timeout:
                    results[size_name]["status"] = "timeout_source"
                    print("  TIMEOUT waiting for file in source")
                    break
                time.sleep(0.1)
            else:
                # File exists - wait for it to be fully written
                while source_file.stat().st_size < size_bytes:
                    if time.monotonic() - file_start > file_timeout:
                        results[size_name]["status"] = "timeout_source_incomplete"
                        print("  TIMEOUT: file incomplete in source")
                        break
                    time.sleep(0.1)

            if results[size_name]["status"] != "pending":
                continue

            # Record when we detected complete file in source
            source_detected = time.monotonic()
            print("  Arrived in source")

            # Check propagation to other producers
            propagation_timeout = 120.0  # 2 min per file for propagation
            all_propagated = True

            for target_name, target_path in other_producers.items():
                target_file = target_path / filename
                prop_start = time.monotonic()

                while not target_file.exists():
                    if time.monotonic() - prop_start > propagation_timeout:
                        results[size_name]["status"] = f"timeout_{target_name}"
                        print(f"  TIMEOUT: did not propagate to {target_name}")
                        all_propagated = False
                        break
                    time.sleep(0.1)

                if not all_propagated:
                    break

                # Verify size
                target_size = target_file.stat().st_size
                if target_size != size_bytes:
                    results[size_name]["status"] = f"size_mismatch_{target_name}"
                    print(
                        f"  SIZE MISMATCH in {target_name}: {target_size} != {size_bytes}"
                    )
                    all_propagated = False
                    break

                # Detection-based timing: time from source detection to target detection
                target_detected = time.monotonic()
                prop_s = target_detected - source_detected
                ctx.metrics.record(f"propagate_{target_name}_{size_name}_s", prop_s)
                print(f"  Propagated to {target_name} in {prop_s:.2f}s")

            if all_propagated:
                results[size_name]["status"] = "success"
                total_s = time.monotonic() - file_start
                ctx.metrics.record(f"total_{size_name}_s", total_s)

        # Print summary
        print("\n" + "=" * 50)
        print("LARGE FILE TEST RESULTS")
        print("=" * 50)

        for size_name, result in results.items():
            status = result["status"]
            size_gb = result["size_bytes"] / GB
            if status == "success":
                print(f"  {size_name} ({size_gb:.0f}GB): OK")
            else:
                print(f"  {size_name} ({size_gb:.0f}GB): FAILED ({status})")

        # Check for failures
        failures = [s for s, r in results.items() if r["status"] != "success"]
        if failures:
            raise AssertionError(
                f"Large file test failed for sizes: {', '.join(failures)}"
            )
        else:
            print("\nAll sizes passed")
