# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Read-write permission test scenario.

Tests:
- F-001: File Create Sync - new file syncs to other producers
- F-007: Multi-Producer Propagation - file propagates to all other producers
- RW mount allows both reading and writing

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s basic.rw --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s basic.rw --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_FILE = TEST_FILE_PREFIX + "rw_test.txt"
TEST_CONTENT = b"RW test content - read write permission test"


class RwScenario:
    """Test read-write mount permissions and multi-producer sync."""

    def write(self, ctx: TestContext) -> None:
        """Write file and read it back (both must succeed)."""
        # F-001: File Create Sync
        print("[F-001] Writing test file...")
        ctx.create_test_file(TEST_FILE, content=TEST_CONTENT)

        print("Reading back test file...")
        read_content = ctx.read_file(TEST_FILE)

        assert read_content == TEST_CONTENT, (
            "Read content does not match written content"
        )
        print("RW permission test passed: write and read both succeeded")

    def verify(self, ctx: TestContext) -> None:
        """Verify file propagated to all producers (run on host)."""
        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Find producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        if not producer_dirs:
            raise RuntimeError(f"No producer directories in {share_dir}")

        print(f"Found {len(producer_dirs)} producers: {list(producer_dirs.keys())}")

        # Wait for file to appear in any producer (source)
        print(f"\nWaiting for {TEST_FILE}...")
        start_time = time.monotonic()
        source_name = None
        source_dir = None

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / TEST_FILE).exists():
                    source_dir = path
                    source_name = name
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError(f"Timeout waiting for {TEST_FILE} in any producer")
            time.sleep(0.1)

        print(f"  Found in source producer: {source_name}")

        # Verify content in source
        content = (source_dir / TEST_FILE).read_bytes()
        assert content == TEST_CONTENT, "Content mismatch in source producer"

        # F-007: Multi-producer sync - verify file propagates to ALL other producers
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if other_producers:
            print(
                f"\n[F-007] Checking propagation to {len(other_producers)} other producer(s)..."
            )

            for name, path in other_producers.items():
                target_file = path / TEST_FILE
                timeout = time.monotonic() + 30.0

                while not target_file.exists() and time.monotonic() < timeout:
                    time.sleep(0.1)

                if target_file.exists():
                    # Verify content matches
                    content = target_file.read_bytes()
                    assert content == TEST_CONTENT, (
                        f"Content mismatch in producer {name}"
                    )
                    print(f"  OK: {name} received file")
                else:
                    raise AssertionError(
                        f"File did not propagate to producer {name} (timeout)"
                    )

            print("  Multi-producer propagation: PASS")
        else:
            raise RuntimeError(
                "F-007: No other producers to verify propagation (need at least 2)"
            )

        print("\nRW verification passed")
