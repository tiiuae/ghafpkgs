# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Write-only (diode) mode test scenario.

Diode mode means one-direction sync: files propagate OUT from diode,
but files from other producers don't propagate IN to diode.

Tests:
- D-001: Diode can write files (propagates to others)
- D-002: Diode doesn't receive from others (no foreign files appear)
- D-003: Diode ignores existing files (can't overwrite trusted files)
- D-004: Diode deletions not propagated (file stays in other producers)
- D-005: Diode renames not propagated (original name stays in other producers)

Important: Start verifier BEFORE writer for D-003 test.

Usage:
    # Host verifier (start first - creates existing file for D-003):
    ghaf-virtiofs-test run /mnt/basePath -s basic.wo --verify

    # VM with wo mount (start after verifier):
    ghaf-virtiofs-test run /mnt/share -s basic.wo --write
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_FILE_1 = TEST_FILE_PREFIX + "wo_test_1.txt"
DELETE_TEST_FILE = TEST_FILE_PREFIX + "wo_delete_test.txt"
RENAME_TEST_FILE = TEST_FILE_PREFIX + "wo_rename_test.txt"
RENAME_NEW_NAME = TEST_FILE_PREFIX + "wo_renamed.txt"
EXISTING_FILE = TEST_FILE_PREFIX + "wo_existing.txt"
FOREIGN_FILE = TEST_FILE_PREFIX + "wo_foreign.txt"
TEST_CONTENT = b"Diode test content"
DELETE_TEST_CONTENT = b"Content for delete propagation test"
RENAME_TEST_CONTENT = b"Content for rename propagation test"
EXISTING_CONTENT = b"TRUSTED content - diode should not overwrite this"
DIODE_ATTEMPT_CONTENT = b"DIODE attempt - this should be ignored"
FOREIGN_CONTENT = b"Foreign content - diode should not receive this"


class WoScenario:
    """Test write-only (diode) mount permissions."""

    def write(self, ctx: TestContext) -> None:
        """Test diode behavior: write propagates out, doesn't receive in."""
        results = {
            "d001_write": False,
            "d002_no_foreign": False,
            "d003_existing_skip": False,
            "d004_delete_local": False,
            "d005_rename_local": False,
        }

        # D-001: Diode can write files
        print("[D-001] Writing test file...")
        ctx.create_test_file(TEST_FILE_1, content=TEST_CONTENT)
        results["d001_write"] = True
        print(f"  OK: Created {TEST_FILE_1}")

        # D-004: Create file, then delete it locally (deletion should not propagate)
        print("\n[D-004] Testing delete non-propagation...")
        delete_file = ctx.path / DELETE_TEST_FILE
        ctx.create_test_file(DELETE_TEST_FILE, content=DELETE_TEST_CONTENT)
        print(f"  Created {DELETE_TEST_FILE}")
        time.sleep(2.0)  # Wait for gate to sync
        delete_file.unlink()
        results["d004_delete_local"] = True
        print("  Deleted locally (verifier will check it still exists elsewhere)")

        # D-005: Create file, then rename it locally (rename should not propagate)
        print("\n[D-005] Testing rename non-propagation...")
        rename_file = ctx.path / RENAME_TEST_FILE
        ctx.create_test_file(RENAME_TEST_FILE, content=RENAME_TEST_CONTENT)
        print(f"  Created {RENAME_TEST_FILE}")
        time.sleep(2.0)  # Wait for gate to sync
        rename_file.rename(ctx.path / RENAME_NEW_NAME)
        results["d005_rename_local"] = True
        print(
            f"  Renamed locally to {RENAME_NEW_NAME} (verifier will check original exists)"
        )

        # D-002: Check foreign file not received (verifier creates it in another producer)
        print("\n[D-002] Checking for foreign file from other producer...")
        time.sleep(1.0)  # Brief wait for any sync
        foreign_file_path = ctx.path / FOREIGN_FILE

        if not foreign_file_path.exists():
            results["d002_no_foreign"] = True
            print(f"  OK: {FOREIGN_FILE} not present (diode not receiving)")
        else:
            print(f"  FAIL: {FOREIGN_FILE} appeared (diode received foreign file)")
            results["d002_no_foreign"] = False

        # D-003: Try to write file that should already exist (created by verifier)
        print("\n[D-003] Testing existing file protection...")
        time.sleep(1.0)  # Brief wait

        print(f"  Attempting to create {EXISTING_FILE} (gate should skip if exists)...")
        ctx.create_test_file(EXISTING_FILE, content=DIODE_ATTEMPT_CONTENT)
        results["d003_existing_skip"] = True  # Verification happens on host
        print("  Write completed (verifier will check trusted content preserved)")

        # Summary
        print("\n" + "=" * 50)
        print("DIODE TEST RESULTS")
        print("=" * 50)
        for test, passed in results.items():
            print(f"  {test}: {'PASS' if passed else 'FAIL'}")

        if all(results.values()):
            print("\nDiode write test passed")
        else:
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"Diode tests failed: {failed}")

    def verify(self, ctx: TestContext) -> None:
        """Verify diode file propagated, check diode isolation (run on host)."""
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

        # Identify diode producer from source_vm if provided
        diode_name = ctx.source_vm
        if diode_name and diode_name in producer_dirs:
            diode_dir = producer_dirs[diode_name]
            print(f"Diode producer (from config): {diode_name}")
        else:
            diode_name = None
            diode_dir = None

        # Find another producer (could be another diode) to create test files
        other_producers = [n for n in sorted(producer_dirs.keys()) if n != diode_name]
        if other_producers:
            other_producer_name = other_producers[0]
        else:
            other_producer_name = sorted(producer_dirs.keys())[0]
        other_producer_dir = producer_dirs[other_producer_name]

        # D-003 Setup: Create existing file in other producer (before diode writes)
        existing_file_path = other_producer_dir / EXISTING_FILE
        print(f"\n[D-003 Setup] Creating existing file in {other_producer_name}...")
        existing_file_path.write_bytes(EXISTING_CONTENT)
        print(f"  Created {EXISTING_FILE} with trusted content")

        # D-002 Setup: Create foreign file in other producer
        # If diode is properly isolated, this file should NOT appear in diode's view
        foreign_file_path = other_producer_dir / FOREIGN_FILE
        print(f"\n[D-002 Setup] Creating foreign file in {other_producer_name}...")
        foreign_file_path.write_bytes(FOREIGN_CONTENT)
        print(f"  Created {FOREIGN_FILE} (diode should not receive this)")

        # Wait for first file to appear in diode producer
        print(f"\nWaiting for {TEST_FILE_1} from diode...")

        if diode_dir is not None:
            # We know the diode producer, wait for file there
            while not (diode_dir / TEST_FILE_1).exists():
                if time.monotonic() - start_time > 20.0:
                    raise TimeoutError(f"Timeout waiting for {TEST_FILE_1}")
                time.sleep(0.1)
            print(f"  File arrived in diode producer: {diode_name}")
        else:
            # Auto-detect diode producer (fallback for manual runs)
            while diode_dir is None:
                for name, path in producer_dirs.items():
                    if (path / TEST_FILE_1).exists():
                        diode_dir = path
                        diode_name = name
                        print(f"  Found diode producer: {name}")
                        break
                if time.monotonic() - start_time > 20.0:
                    raise TimeoutError(f"Timeout waiting for {TEST_FILE_1}")
                time.sleep(0.1)

        # Verify content
        content = (diode_dir / TEST_FILE_1).read_bytes()
        assert content == TEST_CONTENT, "Content mismatch"
        print("  Content verified")

        # Other producers (for later checks)
        other_producers = {n: p for n, p in producer_dirs.items() if n != diode_name}

        # D-001: Verify propagation to at least one other producer
        # Note: other producers may also be diodes, so we only require ONE to receive
        print("\n[D-001] Verifying propagation...")

        if not other_producers:
            raise RuntimeError(
                "D-001: No other producers to verify propagation (need at least 2)"
            )

        # Check if file appears in ANY other producer
        found_in = None
        timeout = time.monotonic() + 10.0
        while not found_in and time.monotonic() < timeout:
            for name, path in other_producers.items():
                if (path / TEST_FILE_1).exists():
                    found_in = name
                    break
            time.sleep(0.1)

        if found_in:
            print(f"  OK: Propagated to {found_in}")
        else:
            raise AssertionError("D-001: File not propagated to any other producer")

        # D-002: Verify diode doesn't receive foreign file from other producer
        print("\n[D-002] Checking diode isolation...")
        time.sleep(1.0)

        # Check that FOREIGN_FILE (created in other producer) did NOT appear in diode share
        diode_foreign_path = diode_dir / FOREIGN_FILE
        if not diode_foreign_path.exists():
            print(f"  OK: {FOREIGN_FILE} not in diode share (isolation working)")
        else:
            raise AssertionError(
                f"Diode received {FOREIGN_FILE} from other producer (should be isolated)"
            )

        # D-003: Verify existing file was NOT overwritten by diode
        print("\n[D-003] Checking existing file preserved...")
        time.sleep(2.0)  # Wait for diode's attempt to be processed

        # Check the file in the trusted producer (where we created it)
        if existing_file_path.exists():
            content = existing_file_path.read_bytes()
            if content == EXISTING_CONTENT:
                print(f"  OK: {EXISTING_FILE} has original trusted content")
            elif content == DIODE_ATTEMPT_CONTENT:
                raise AssertionError(
                    "Diode overwrote existing file (should be skipped)"
                )
            else:
                raise AssertionError(f"{EXISTING_FILE} has unexpected content")
        else:
            raise AssertionError(f"{EXISTING_FILE} was deleted")

        # D-004: Verify diode deletion not propagated
        # VM created DELETE_TEST_FILE then deleted it - check it still exists elsewhere
        print("\n[D-004] Verifying diode deletion not propagated...")

        # Wait for file to appear in at least one other producer
        delete_file_found = {}
        timeout = time.monotonic() + 10.0
        while not delete_file_found and time.monotonic() < timeout:
            for name, path in other_producers.items():
                if (path / DELETE_TEST_FILE).exists():
                    delete_file_found[name] = path / DELETE_TEST_FILE
            time.sleep(0.1)

        if not delete_file_found:
            raise AssertionError("D-004: File never appeared in other producers")

        print(f"  File appeared in: {list(delete_file_found.keys())}")

        # Wait for VM to delete (file disappears from diode share)
        diode_delete_file = diode_dir / DELETE_TEST_FILE
        timeout = time.monotonic() + 10.0
        while diode_delete_file.exists() and time.monotonic() < timeout:
            time.sleep(0.1)

        if not diode_delete_file.exists():
            print("  VM deleted file from diode share")

            # Verify file still exists in all found locations
            time.sleep(1.0)
            for loc_name, file_path in delete_file_found.items():
                if file_path.exists():
                    print(f"  OK: File still in {loc_name} (deletion not propagated)")
                else:
                    raise AssertionError(f"Deletion propagated to {loc_name}")
        else:
            raise AssertionError("D-004: VM didn't delete file from diode share")

        # D-005: Verify diode rename not propagated
        print("\n[D-005] Verifying diode rename not propagated...")

        # Wait for file to appear in at least one other producer
        rename_file_found = {}
        timeout = time.monotonic() + 10.0
        while not rename_file_found and time.monotonic() < timeout:
            for name, path in other_producers.items():
                if (path / RENAME_TEST_FILE).exists():
                    rename_file_found[name] = path / RENAME_TEST_FILE
            time.sleep(0.1)

        if not rename_file_found:
            raise AssertionError("D-005: File never appeared in other producers")

        print(f"  File appeared in: {list(rename_file_found.keys())}")

        # Wait for VM to rename (original disappears, new appears in diode share)
        diode_orig = diode_dir / RENAME_TEST_FILE
        diode_renamed = diode_dir / RENAME_NEW_NAME
        timeout = time.monotonic() + 10.0
        while diode_orig.exists() and time.monotonic() < timeout:
            time.sleep(0.1)

        if not diode_orig.exists() and diode_renamed.exists():
            print(f"  VM renamed file to {RENAME_NEW_NAME}")

            # Verify original still exists in found locations (rename not propagated)
            time.sleep(1.0)
            for loc_name, file_path in rename_file_found.items():
                if file_path.exists():
                    print(f"  OK: Original still in {loc_name} (rename not propagated)")
                else:
                    # Check if renamed version appeared instead
                    loc_path = file_path.parent
                    if (loc_path / RENAME_NEW_NAME).exists():
                        raise AssertionError(f"Rename propagated to {loc_name}")
                    else:
                        raise AssertionError(f"File disappeared from {loc_name}")
        else:
            raise AssertionError("D-005: VM didn't rename file in diode share")

        print("\nWO verification passed: diode behavior confirmed")
