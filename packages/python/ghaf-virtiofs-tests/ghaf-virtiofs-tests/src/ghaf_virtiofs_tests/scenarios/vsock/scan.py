# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""vsock client on-write scan test scenario.

Tests:
- VS-001: Clean file passes vclient scan (accessible after write)
- VS-002: Infected file blocked by vclient scan

Tests the clamd-vclient on-write scanning integration.
Different from basic.scan which tests gate-level scanning on host.

Usage:
    # VM with vclient-monitored share:
    ghaf-virtiofs-test run /mnt/share -s vsock.scan --write

    # Host verifier (optional - checks propagation):
    ghaf-virtiofs-test run /mnt/basePath -s vsock.scan --verify
"""

import time

from ...context import EICAR_STRING, TEST_FILE_PREFIX, TestContext

CLEAN_FILE = TEST_FILE_PREFIX + "vsock_scan_clean.txt"
INFECTED_FILE = TEST_FILE_PREFIX + "vsock_scan_infected.txt"
CLEAN_CONTENT = b"Clean content for vclient scan test"


class ScanScenario:
    """Test vclient on-write scanning."""

    def write(self, ctx: TestContext) -> None:
        """Write clean and infected files, verify vclient scanning."""
        results = {
            "clean_written": False,
            "clean_accessible": False,
            "infected_written": False,
            "infected_blocked": False,
        }

        # VS-001: Clean file should pass scan
        print("[VS-001] Writing clean file...")
        try:
            ctx.create_test_file(CLEAN_FILE, content=CLEAN_CONTENT)
            results["clean_written"] = True
            print(f"  Created: {CLEAN_FILE}")

            # Wait for vclient to scan
            time.sleep(2.0)

            # Verify file is still accessible
            clean_path = ctx.path / CLEAN_FILE
            if clean_path.exists():
                content = clean_path.read_bytes()
                if content == CLEAN_CONTENT:
                    results["clean_accessible"] = True
                    print("  OK: Clean file accessible after scan")
                else:
                    raise AssertionError("Clean file content mismatch")
            else:
                raise AssertionError("Clean file not accessible")
        except Exception as e:
            print(f"  Error: {e}")

        # VS-002: Infected file should be blocked
        print("\n[VS-002] Writing infected file...")
        try:
            infected_path = ctx.path / INFECTED_FILE
            infected_path.write_bytes(EICAR_STRING)
            results["infected_written"] = True
            print(f"  Created: {INFECTED_FILE}")

            # Wait for vclient to scan and act
            time.sleep(3.0)

            # Check if file is blocked/removed/quarantined
            if not infected_path.exists():
                results["infected_blocked"] = True
                print("  OK: Infected file removed by vclient")
            else:
                # File exists - check if access is blocked
                try:
                    content = infected_path.read_bytes()
                    if len(content) == 0:
                        results["infected_blocked"] = True
                        print("  OK: Infected file emptied by vclient")
                    else:
                        raise AssertionError("Infected file still accessible")
                except (PermissionError, OSError) as e:
                    results["infected_blocked"] = True
                    print(f"  OK: Infected file access blocked: {e}")
        except Exception as e:
            print(f"  Error: {e}")

        # Summary
        print("\n" + "=" * 50)
        print("VSOCK CLIENT SCAN RESULTS")
        print("=" * 50)
        print(
            f"  VS-001 Clean written: {'PASS' if results['clean_written'] else 'FAIL'}"
        )
        print(
            f"  VS-001 Clean accessible: {'PASS' if results['clean_accessible'] else 'FAIL'}"
        )
        print(
            f"  VS-002 Infected written: {'PASS' if results['infected_written'] else 'FAIL'}"
        )
        print(
            f"  VS-002 Infected blocked: {'PASS' if results['infected_blocked'] else 'FAIL'}"
        )

        # Core requirements
        if results["clean_accessible"] and results["infected_blocked"]:
            print("\nvclient scan test passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"vclient scan test failed: {failed}")

    def verify(self, ctx: TestContext) -> None:
        """Host verify not needed - vclient scanning is tested in VM."""
        print("vsock.scan: Host verification not required")
        print("vclient on-write scanning is validated by the write test in VM")
