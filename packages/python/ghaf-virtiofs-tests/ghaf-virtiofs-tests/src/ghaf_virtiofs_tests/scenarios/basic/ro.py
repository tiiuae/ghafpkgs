# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Read-only permission test scenario.

Tests:
- R-001: Consumer can read file created directly in export
- R-002: Consumer sees file propagated via producer share (full pipeline)
- R-003: Consumer cannot write to export

Usage:
    # VM with ro mount (consumer):
    ghaf-virtiofs-test run /mnt/share -s basic.ro --write

    # Host verifier (monitors basePath, creates test files):
    ghaf-virtiofs-test run /mnt/basePath -s basic.ro --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_FILE = TEST_FILE_PREFIX + "ro_test.txt"
CONSUMER_TEST_FILE = TEST_FILE_PREFIX + "ro_consumer_test.txt"
CONSUMER_TEST_CONTENT = b"Content for consumer read-only access test"
PRODUCER_TEST_FILE = TEST_FILE_PREFIX + "ro_producer_test.txt"
PRODUCER_TEST_CONTENT = b"Content written via producer share for R-002"


class RoScenario:
    """Test read-only mount permissions and consumer access."""

    def write(self, ctx: TestContext) -> None:
        """Test RO mount: write blocked, read from export-ro allowed."""

        results = {
            "write_blocked": False,
            "consumer_read": False,
            "producer_propagated": False,
        }

        # R-003: Write should fail on ro mount
        print("[R-003] Attempting to write on ro mount (should fail)...")
        try:
            ctx.create_test_file(TEST_FILE, content=b"This should fail")
            raise AssertionError("Write succeeded on ro mount (should have failed)")
        except (OSError, PermissionError) as e:
            results["write_blocked"] = True
            print(f"  OK: Write correctly rejected: {e}")

        # R-001: Consumer read from export-ro (if available)
        # The verifier creates a file in export, consumer should see it in export-ro
        print("\n[R-001] Checking for consumer test file...")

        # Wait for file to appear (verifier creates it)
        timeout = time.monotonic() + 30.0
        consumer_file = ctx.path / CONSUMER_TEST_FILE

        while not consumer_file.exists() and time.monotonic() < timeout:
            time.sleep(0.5)

        if not consumer_file.exists():
            raise AssertionError(f"{CONSUMER_TEST_FILE} not found")

        try:
            content = consumer_file.read_bytes()
            if content == CONSUMER_TEST_CONTENT:
                results["consumer_read"] = True
                print(f"  OK: Read {CONSUMER_TEST_FILE} with correct content")
            else:
                raise AssertionError("Content mismatch")
        except (OSError, PermissionError) as e:
            raise AssertionError(f"Cannot read file: {e}") from e

        # R-002: Consumer sees file written via producer share (full pipeline)
        print("\n[R-002] Checking for file propagated via producer share...")
        producer_file = ctx.path / PRODUCER_TEST_FILE
        timeout = time.monotonic() + 30.0

        while not producer_file.exists() and time.monotonic() < timeout:
            time.sleep(0.5)

        if not producer_file.exists():
            raise AssertionError(
                f"{PRODUCER_TEST_FILE} not found (gate propagation failed?)"
            )

        try:
            content = producer_file.read_bytes()
            if content == PRODUCER_TEST_CONTENT:
                results["producer_propagated"] = True
                print(f"  OK: Read {PRODUCER_TEST_FILE} from producer via gate")
            else:
                raise AssertionError("Content mismatch")
        except (OSError, PermissionError) as e:
            raise AssertionError(f"Cannot read file: {e}") from e

        # R-003: Verify consumer cannot write/modify
        print("\n[R-003] Attempting to modify consumer test file (should fail)...")
        try:
            consumer_file.write_bytes(b"Modified content")
            raise AssertionError("Write succeeded (should have failed)")
        except (OSError, PermissionError) as e:
            print(f"  OK: Write correctly rejected: {e}")

        # Summary
        print("\n" + "=" * 50)
        print("RO PERMISSION TEST RESULTS")
        print("=" * 50)
        print(
            f"  R-001 Consumer read (direct): {'PASS' if results['consumer_read'] else 'FAIL'}"
        )
        print(
            f"  R-002 Producer propagated: {'PASS' if results['producer_propagated'] else 'FAIL'}"
        )
        print(
            f"  R-003 Write blocked: {'PASS' if results['write_blocked'] else 'FAIL'}"
        )

        if all(results.values()):
            print("\nRO permission test passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"RO permission test failed: {failed}")

    def verify(self, ctx: TestContext) -> None:
        """Verify no file appeared, set up consumer test file (run on host)."""

        base_path = ctx.path
        share_dir = base_path / "share"
        export_dir = base_path / "export"
        export_ro_dir = base_path / "export-ro"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        # Find producer directories
        producer_dirs = {d.name: d for d in share_dir.iterdir() if d.is_dir()}
        print(f"Found producers: {list(producer_dirs.keys())}")

        # R-001 Setup: Create consumer test file directly in export
        if export_dir.exists():
            print(f"\n[R-001 Setup] Creating {CONSUMER_TEST_FILE} in export...")
            consumer_file = export_dir / CONSUMER_TEST_FILE
            consumer_file.write_bytes(CONSUMER_TEST_CONTENT)
            print(f"  Created: {consumer_file}")

            if export_ro_dir.exists():
                print("  export-ro/ directory exists (consumer VMs use this)")
            else:
                print(
                    "  INFO: export-ro/ not found (consumers may use export/ directly)"
                )
        else:
            raise RuntimeError(f"export/ directory not found in {base_path}")

        # R-002 Setup: Write file to producer share (gate propagates to export)
        if not producer_dirs:
            raise RuntimeError("No producer directories found for R-002 test")

        # Pick first producer for the test
        producer_name = next(iter(producer_dirs))
        producer_dir = producer_dirs[producer_name]
        print(
            f"\n[R-002 Setup] Creating {PRODUCER_TEST_FILE} in producer {producer_name}..."
        )
        producer_file = producer_dir / PRODUCER_TEST_FILE
        producer_file.write_bytes(PRODUCER_TEST_CONTENT)
        print(f"  Created: {producer_file}")
        print("  Gate will propagate this to export for consumer to read")

        # Wait a bit to ensure nothing propagates from RO mount
        wait_time = 5.0
        print(f"\nWaiting {wait_time}s to verify no file appears from RO mount...")
        time.sleep(wait_time)

        # Check that file does NOT exist anywhere (from the RO VM's write attempt)
        for name, path in producer_dirs.items():
            if (path / TEST_FILE).exists():
                raise AssertionError(f"File should not exist in producer {name}")

        if export_dir.exists() and (export_dir / TEST_FILE).exists():
            raise AssertionError("File should not exist in export")

        print("RO verification passed: no file propagated from RO mount")
