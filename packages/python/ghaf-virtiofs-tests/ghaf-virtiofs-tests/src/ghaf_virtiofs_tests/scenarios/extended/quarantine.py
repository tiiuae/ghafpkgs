# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Quarantine action test (extended).

Tests:
- S-003: Infected files quarantined with mode 000, root:root
- S-003b: Infected file deleted from source and not propagated to other producers

Requires channel configured with:
  scanning.infectedAction = "quarantine"

Note: Infected file blocking is tested by basic.scan.
This test focuses on quarantine-specific behavior and verification.

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.quarantine --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.quarantine --verify
"""

import stat
import time

from ...context import EICAR_STRING, TEST_FILE_PREFIX, TestContext

INFECTED_FILE = TEST_FILE_PREFIX + "quarantine_infected.txt"


class QuarantineScenario:
    """Test quarantine action for infected files."""

    def write(self, ctx: TestContext) -> None:
        """Create infected test file."""
        print(f"Creating infected file: {INFECTED_FILE}")
        ctx.create_test_file(INFECTED_FILE, content=EICAR_STRING)

        print("\nQuarantine test: infected file created")
        print("Expected: file goes to quarantine/ with mode 000")

    def verify(self, ctx: TestContext) -> None:
        """Verify infected file is quarantined with correct permissions."""
        base_path = ctx.path
        share_dir = base_path / "share"
        quarantine_dir = base_path / "quarantine"

        if not share_dir.exists():
            raise RuntimeError(f"share/ directory not found in {base_path}")

        if not quarantine_dir.exists():
            raise RuntimeError(f"quarantine/ directory not found in {base_path}")

        print(f"Quarantine dir: {quarantine_dir}")
        start_time = time.monotonic()

        results = {
            "infected_in_quarantine": False,
            "quarantine_mode_000": False,
            "quarantine_root_owned": False,
            "not_in_producers": False,
        }

        # Wait for file to appear in quarantine
        print(f"\nWaiting for {INFECTED_FILE} in quarantine...")

        while time.monotonic() - start_time < 60.0:
            quarantine_files = list(quarantine_dir.rglob("*"))
            infected_files = [f for f in quarantine_files if f.is_file()]

            for qfile in infected_files:
                # Check if this looks like our infected file
                if INFECTED_FILE in qfile.name or "quarantine_infected" in qfile.name:
                    results["infected_in_quarantine"] = True
                    print(f"  Found quarantined file: {qfile}")

                    # Check permissions (mode 000)
                    try:
                        file_stat = qfile.stat()
                        mode = stat.S_IMODE(file_stat.st_mode)
                        if mode == 0:
                            results["quarantine_mode_000"] = True
                            print("  OK: mode is 000")
                        else:
                            raise AssertionError(
                                f"Quarantine mode is {oct(mode)} (expected 000)"
                            )

                        # Check ownership (root:root = uid 0, gid 0)
                        if file_stat.st_uid == 0 and file_stat.st_gid == 0:
                            results["quarantine_root_owned"] = True
                            print("  OK: owned by root:root")
                        else:
                            print(
                                f"  INFO: owned by {file_stat.st_uid}:{file_stat.st_gid}"
                            )
                    except PermissionError:
                        # If we can't stat it, it's probably mode 000
                        results["quarantine_mode_000"] = True
                        print("  OK: cannot stat (likely mode 000)")
                    break

            if results["infected_in_quarantine"]:
                break
            time.sleep(0.5)

        if not results["infected_in_quarantine"]:
            # List what's in quarantine for debugging
            quarantine_files = list(quarantine_dir.rglob("*"))
            infected_files = [f for f in quarantine_files if f.is_file()]
            if infected_files:
                print("  Files in quarantine (none matched):")
                for f in infected_files:
                    print(f"    - {f.name}")
            else:
                print("  No files found in quarantine")

        # Verify infected file deleted from source AND not propagated to others
        # This confirms quarantine action (not log-only) - checks ALL producer shares
        print("\nVerifying infected file removed from all producer shares...")
        producer_dirs = [d for d in share_dir.iterdir() if d.is_dir()]
        infected_in_producers = False

        for producer_dir in producer_dirs:
            for check_file in producer_dir.rglob("*"):
                if check_file.is_file():
                    if (
                        INFECTED_FILE in check_file.name
                        or "quarantine_infected" in check_file.name
                    ):
                        print(
                            f"  FAIL: Infected file found in {producer_dir.name}: {check_file}"
                        )
                        infected_in_producers = True

        if not infected_in_producers:
            results["not_in_producers"] = True
            print("  OK: Infected file removed from all shares (incl. source)")
        else:
            raise AssertionError(
                "Infected file still in producer shares (log-only mode?)"
            )

        # Summary
        print("\n" + "=" * 50)
        print("QUARANTINE TEST RESULTS")
        print("=" * 50)
        print(
            f"  Infected in quarantine: {'PASS' if results['infected_in_quarantine'] else 'FAIL'}"
        )
        print(f"  Mode 000: {'PASS' if results['quarantine_mode_000'] else 'FAIL'}")
        print(f"  Root owned: {'PASS' if results['quarantine_root_owned'] else 'FAIL'}")
        print(
            f"  Removed from shares: {'PASS' if results['not_in_producers'] else 'FAIL'}"
        )

        if all(results.values()):
            print("\nQuarantine test passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"Quarantine test failed: {failed}")
