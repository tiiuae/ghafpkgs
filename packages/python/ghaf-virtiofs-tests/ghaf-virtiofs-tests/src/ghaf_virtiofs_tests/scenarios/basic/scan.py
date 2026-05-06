# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Virus scanning test scenario.

Tests:
- S-001: Clean File Pass - clean files sync to other producers
- S-002: EICAR Block - malware is blocked from propagation

Host runs as verifier to observe gate behavior directly.
No checkpoint coordination - both sides run independently.

Usage:
    # Writer VM:
    ghaf-virtiofs-test run /mnt/share -s basic.scan -c /tmp --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s basic.scan -c /tmp --verify
"""

import time

from ...context import EICAR_STRING, TEST_FILE_PREFIX, TestContext

CLEAN_FILE = TEST_FILE_PREFIX + "scan_clean.txt"
INFECTED_FILE = TEST_FILE_PREFIX + "scan_infected.txt"


class ScanScenario:
    """Test virus scanning - clean propagates, infected blocked."""

    def write(self, ctx: TestContext) -> None:
        """Create clean and infected test files."""
        # S-001: Clean File Pass
        print("[S-001] Creating clean file...")
        ctx.create_test_file(CLEAN_FILE, content=b"This is clean content.")

        # S-002: EICAR Block
        print("[S-002] Creating infected file with EICAR test string...")
        ctx.create_test_file(INFECTED_FILE, content=EICAR_STRING)
        print(f"  Created with {len(EICAR_STRING)} bytes")

        print("Files created. Clean should propagate, infected should be blocked.")

    def verify(self, ctx: TestContext) -> None:
        """Verify clean propagates and infected is blocked (run on host).

        The scanner blocks infected files before they reach the share directory,
        so we only wait for the clean file to appear and verify the infected
        file never appears anywhere.
        """
        base_path = ctx.path
        share_dir = base_path / "share"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Discover producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        if not producer_dirs:
            raise RuntimeError(f"No producer directories in {share_dir}")

        print(f"Found producers: {list(producer_dirs.keys())}")

        # Detect source producer (where clean file appears)
        source_name = None
        start_time = time.monotonic()

        print("\n[S-001] Waiting for clean file to appear...")
        while source_name is None:
            for name, path in producer_dirs.items():
                if (path / CLEAN_FILE).exists():
                    source_name = name
                    break
            if time.monotonic() - start_time > 60.0:
                raise TimeoutError("Timeout waiting for clean file")
            time.sleep(0.01)

        print(f"  Source producer: {source_name}")

        # Check other producers exist
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "S-001: No other producers to verify propagation (need at least 2)"
            )

        # Wait for clean file to propagate to other producers
        print(
            f"\n[S-001] Checking propagation to {len(other_producers)} other producer(s)..."
        )

        for name, path in other_producers.items():
            clean_target = path / CLEAN_FILE
            timeout = time.monotonic() + 30.0
            while not clean_target.exists() and time.monotonic() < timeout:
                time.sleep(0.01)
            if not clean_target.exists():
                raise AssertionError(f"Clean file did not propagate to {name}")
            print(f"  OK: {name} has clean file")

        # Verify infected file is blocked in all producers
        # Wait a bit to ensure scanner had time to process
        block_verify_time = 5.0
        print(
            f"\n[S-002] Verifying infected file is blocked (waiting {block_verify_time}s)..."
        )
        time.sleep(block_verify_time)

        for name, path in producer_dirs.items():
            infected_path = path / INFECTED_FILE
            if infected_path.exists():
                raise AssertionError(
                    f"S-002: Infected file should be blocked but found in {name}"
                )
            print(f"  OK: {name} blocked infected file")

        print("\nScan test passed: clean propagated, infected blocked")
