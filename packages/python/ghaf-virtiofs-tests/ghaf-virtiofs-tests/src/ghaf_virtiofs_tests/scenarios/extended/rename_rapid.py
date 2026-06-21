# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Rapid rename cycles test scenario (extended).

Tests:
- F-018: Rapid Rename Cycles - file renamed multiple times in sequence

This tests the edge case where a file is renamed rapidly (a->b->c->d->final).
This pattern occurs with browser downloads, editor save operations, and
other real-world workflows through virtiofsd.

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.rename_rapid --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.rename_rapid --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

# Test file names for rapid rename sequence
RENAME_BASE = TEST_FILE_PREFIX + "rapid_rename"
RENAME_STAGES = ["_a.tmp", "_b.tmp", "_c.tmp", "_d.tmp", "_final.txt"]
TEST_CONTENT = b"Content through rapid rename cycle"


class RenameRapidScenario:
    """Test rapid sequential file renames (F-018)."""

    def write(self, ctx: TestContext) -> None:
        """Create file and rename it through multiple stages rapidly."""
        # Create initial file
        initial_name = RENAME_BASE + RENAME_STAGES[0]
        print(f"[F-018] Creating {initial_name}...")
        ctx.create_test_file(initial_name, content=TEST_CONTENT)

        # Wait briefly for initial sync
        time.sleep(1.0)

        # Rapid rename cycle: a->b->c->d->final
        current_name = initial_name
        for next_suffix in RENAME_STAGES[1:]:
            next_name = RENAME_BASE + next_suffix
            current_path = ctx.path / current_name
            next_path = ctx.path / next_name

            print(f"  Renaming {current_name} -> {next_name}...")
            current_path.rename(next_path)
            current_name = next_name
            # No delay between renames - testing rapid succession

        final_name = RENAME_BASE + RENAME_STAGES[-1]
        print(f"[F-018] Rapid rename complete: final name is {final_name}")

    def verify(self, ctx: TestContext) -> None:
        """Verify final renamed file propagated correctly (run on host)."""
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

        # Detect source producer - wait for final file
        final_name = RENAME_BASE + RENAME_STAGES[-1]
        source_name = None
        source_dir = None

        print(f"\n[F-018] Waiting for {final_name} to appear...")
        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / final_name).exists():
                    source_dir = path
                    source_name = name
                    print(f"  Found in producer: {name}")
                    break
            if time.monotonic() - start_time > 60.0:
                # Check what files exist for debugging
                for name, path in producer_dirs.items():
                    found = list(path.glob(f"{RENAME_BASE}*"))
                    if found:
                        print(f"  {name}: {[f.name for f in found]}")
                raise TimeoutError(f"Timeout waiting for {final_name}")
            time.sleep(0.1)

        # Check other producers exist
        other_producers = {n: p for n, p in producer_dirs.items() if n != source_name}

        if not other_producers:
            raise RuntimeError(
                "F-018: No other producers to verify propagation (need at least 2)"
            )

        # Verify final file propagated to other producers
        print(
            f"\n[F-018] Verifying propagation to {len(other_producers)} producer(s)..."
        )

        for name, path in other_producers.items():
            target_final = path / final_name
            timeout = time.monotonic() + 30.0

            while not target_final.exists() and time.monotonic() < timeout:
                time.sleep(0.1)

            if not target_final.exists():
                # Check what intermediate files might exist
                found = list(path.glob(f"{RENAME_BASE}*"))
                print(f"  {name}: found {[f.name for f in found]}")
                raise AssertionError(f"Final file not propagated to {name} (timeout)")

            # Verify content preserved through all renames
            content = target_final.read_bytes()
            if content != TEST_CONTENT:
                raise AssertionError(f"Content mismatch in {name}")

            print(f"  OK: {name} has {final_name} with correct content")

        # Verify intermediate files don't exist (they should have been renamed away)
        print("\n[F-018] Verifying no intermediate files remain...")
        for name, path in other_producers.items():
            for intermediate_suffix in RENAME_STAGES[:-1]:
                intermediate_name = RENAME_BASE + intermediate_suffix
                if (path / intermediate_name).exists():
                    print(f"  INFO: {name} has intermediate {intermediate_name}")

        print("\n[F-018] Rapid rename verification passed")
