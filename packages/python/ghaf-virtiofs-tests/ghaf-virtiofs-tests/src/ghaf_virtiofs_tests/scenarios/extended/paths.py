# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Path edge case tests (extended).

Tests:
- F-014: Nested directories - deep paths work
- F-015: Special chars - Unicode filenames
- F-016: Spaces in names - Whitespace handling

Usage:
    # VM with rw mount:
    ghaf-virtiofs-test run /mnt/share -s extended.paths --write

    # Host verifier (monitors basePath):
    ghaf-virtiofs-test run /mnt/basePath -s extended.paths --verify
"""

import time

from ...context import TEST_FILE_PREFIX, TestContext

TEST_CONTENT = b"Path edge case test content"

# F-014: Nested directories
NESTED_PATH = TEST_FILE_PREFIX + "a/b/c/d/e/f/g/h"
NESTED_FILE = TEST_FILE_PREFIX + "nested_deep.txt"

# F-015: Special characters (Unicode)
SPECIAL_CHARS_FILES = [
    TEST_FILE_PREFIX + "unicode_emoji_\U0001f600.txt",  # emoji
    TEST_FILE_PREFIX + "unicode_chinese_\u4e2d\u6587.txt",  # Chinese characters
    TEST_FILE_PREFIX + "unicode_arabic_\u0639\u0631\u0628\u064a.txt",  # Arabic
    TEST_FILE_PREFIX
    + "unicode_cyrillic_\u0440\u0443\u0441\u0441\u043a\u0438\u0439.txt",  # Russian
    TEST_FILE_PREFIX + "special_chars_@#$%^&().txt",  # Common special chars
    TEST_FILE_PREFIX + "special_plus+minus-equals=.txt",  # Math symbols
]

# F-016: Spaces in names
SPACE_FILES = [
    TEST_FILE_PREFIX + "file with spaces.txt",
    TEST_FILE_PREFIX + "  leading spaces.txt",
    TEST_FILE_PREFIX + "trailing spaces  .txt",
    TEST_FILE_PREFIX + "multiple   internal   spaces.txt",
    TEST_FILE_PREFIX + "tab\there.txt",
    TEST_FILE_PREFIX + "dir with spaces/file inside.txt",
]


class PathsScenario:
    """Test path edge cases."""

    def write(self, ctx: TestContext) -> None:
        """Create files with edge case paths."""

        results = {"nested": False, "special": [], "spaces": []}

        # F-014: Nested directories
        print(f"Creating nested path: {NESTED_PATH}/{NESTED_FILE}")
        try:
            nested_dir = ctx.path / NESTED_PATH
            nested_dir.mkdir(parents=True, exist_ok=True)
            (nested_dir / NESTED_FILE).write_bytes(TEST_CONTENT)
            results["nested"] = True
            print("  OK")
        except Exception as e:
            print(f"  FAILED: {e}")

        # F-015: Special characters
        print("\nCreating files with special characters...")
        for filename in SPECIAL_CHARS_FILES:
            try:
                ctx.create_test_file(filename, content=TEST_CONTENT)
                results["special"].append(filename)
                print(f"  OK: {filename}")
            except Exception as e:
                print(f"  FAILED: {filename} - {e}")

        # F-016: Spaces in names
        print("\nCreating files with spaces...")
        for filename in SPACE_FILES:
            try:
                file_path = ctx.path / filename
                parent = file_path.parent
                if parent != ctx.path and not parent.exists():
                    # New subdirectory - delay for watcher to register
                    parent.mkdir(parents=True, exist_ok=True)
                    time.sleep(0.5)
                file_path.write_bytes(TEST_CONTENT)
                results["spaces"].append(filename)
                print(f"  OK: '{filename}'")
            except Exception as e:
                print(f"  FAILED: '{filename}' - {e}")

        print(
            f"\nCreated: nested={results['nested']}, "
            f"special={len(results['special'])}/{len(SPECIAL_CHARS_FILES)}, "
            f"spaces={len(results['spaces'])}/{len(SPACE_FILES)}"
        )

    def verify(self, ctx: TestContext) -> None:
        """Verify edge case paths propagated (run on host)."""

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

        # Wait for nested file to detect source producer
        print("\nWaiting for nested path...")
        source_dir = None
        nested_full = f"{NESTED_PATH}/{NESTED_FILE}"

        while source_dir is None:
            for name, path in producer_dirs.items():
                if (path / nested_full).exists():
                    source_dir = path
                    print(f"  Found source producer: {name}")
                    break
            if time.monotonic() - start_time > 120.0:
                raise TimeoutError("Timeout waiting for nested file")
            time.sleep(0.1)

        # Find other producers for propagation verification
        source_name = [n for n, p in producer_dirs.items() if p == source_dir][0]
        other_producers = {n: p for n, p in producer_dirs.items() if p != source_dir}

        if not other_producers:
            raise RuntimeError(
                "Paths test: No other producers to verify propagation (need at least 2)"
            )

        print(f"Source: {source_name}, targets: {list(other_producers.keys())}")

        results = {"nested": False, "special": [], "spaces": []}

        # F-014: Verify nested path propagated to other producers
        print("\n[F-014] Checking nested directories propagated...")
        for name, path in other_producers.items():
            target_nested = path / nested_full
            timeout = time.monotonic() + 60.0
            while not target_nested.exists() and time.monotonic() < timeout:
                time.sleep(0.1)
            if not target_nested.exists():
                # Debug: check what part of the path exists
                existing_parts = []
                check_path = path
                for part in NESTED_PATH.split("/"):
                    check_path = check_path / part
                    if check_path.exists():
                        existing_parts.append(part)
                    else:
                        break
                if existing_parts:
                    print(f"  DEBUG: Partial path exists: {'/'.join(existing_parts)}")
                else:
                    print(f"  DEBUG: No nested directories exist in {name}")
                raise AssertionError(f"Nested path not propagated to {name}")
            content = target_nested.read_bytes()
            if content != TEST_CONTENT:
                raise AssertionError(f"Nested path content mismatch in {name}")
            print(f"  OK: {nested_full} propagated to {name}")
        results["nested"] = True

        # F-015: Verify special characters propagated to other producers
        print("\n[F-015] Checking special character files propagated...")
        for filename in SPECIAL_CHARS_FILES:
            all_propagated = True
            for name, path in other_producers.items():
                target_file = path / filename
                timeout = time.monotonic() + 30.0
                while not target_file.exists() and time.monotonic() < timeout:
                    time.sleep(0.1)
                if not target_file.exists():
                    print(f"  FAILED: {filename} not propagated to {name}")
                    all_propagated = False
                    break
                content = target_file.read_bytes()
                if content != TEST_CONTENT:
                    print(f"  FAILED: {filename} content mismatch in {name}")
                    all_propagated = False
                    break
            if all_propagated:
                results["special"].append(filename)
                print(f"  OK: {filename}")

        # F-016: Verify spaces in names propagated to other producers
        print("\n[F-016] Checking files with spaces propagated...")
        for filename in SPACE_FILES:
            all_propagated = True
            for name, path in other_producers.items():
                target_file = path / filename
                timeout = time.monotonic() + 30.0
                while not target_file.exists() and time.monotonic() < timeout:
                    time.sleep(0.1)
                if not target_file.exists():
                    print(f"  FAILED: '{filename}' not propagated to {name}")
                    all_propagated = False
                    break
                content = target_file.read_bytes()
                if content != TEST_CONTENT:
                    print(f"  FAILED: '{filename}' content mismatch in {name}")
                    all_propagated = False
                    break
            if all_propagated:
                results["spaces"].append(filename)
                print(f"  OK: '{filename}'")

        # Summary
        print("\n" + "=" * 50)
        print("PATH EDGE CASE RESULTS")
        print("=" * 50)
        print(f"  Nested directories: {'PASS' if results['nested'] else 'FAIL'}")
        print(f"  Special chars: {len(results['special'])}/{len(SPECIAL_CHARS_FILES)}")
        print(f"  Spaces in names: {len(results['spaces'])}/{len(SPACE_FILES)}")

        # Determine overall pass/fail
        total_expected = 1 + len(SPECIAL_CHARS_FILES) + len(SPACE_FILES)
        total_passed = (
            (1 if results["nested"] else 0)
            + len(results["special"])
            + len(results["spaces"])
        )

        if total_passed == total_expected:
            print(f"\nAll {total_expected} tests passed")
        else:
            failed = []
            if not results["nested"]:
                failed.append("nested_directories")
            missing_special = set(SPECIAL_CHARS_FILES) - set(results["special"])
            if missing_special:
                failed.append(f"special_chars({len(missing_special)} failed)")
            missing_spaces = set(SPACE_FILES) - set(results["spaces"])
            if missing_spaces:
                failed.append(f"spaces_in_names({len(missing_spaces)} failed)")
            raise AssertionError(
                f"Path edge case tests failed ({total_passed}/{total_expected}): {failed}"
            )
