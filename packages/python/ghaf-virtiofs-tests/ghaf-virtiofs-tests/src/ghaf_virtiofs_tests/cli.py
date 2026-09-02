# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Command-line interface for virtiofs integration tests."""

import argparse
import importlib
import shutil
import socket
import sys
from pathlib import Path

from .context import TEST_FILE_PREFIX, TestContext


def get_hostname() -> str:
    """Get the system hostname."""
    return socket.gethostname()


def get_scenario(name: str) -> type:
    """Dynamically load a scenario class.

    Args:
        name: Scenario name like "basic.rw" or "extended.large_file"
    """
    module_name = f"ghaf_virtiofs_tests.scenarios.{name}"
    try:
        module = importlib.import_module(module_name)
    except ModuleNotFoundError as e:
        print(f"Error: Scenario '{name}' not found", file=sys.stderr)
        print(f"  Looking for module: {module_name}", file=sys.stderr)
        raise SystemExit(1) from e

    # Class name is based on last part of name (e.g., "rw" from "basic.rw")
    scenario_name = name.split(".")[-1]
    class_name = "".join(word.title() for word in scenario_name.split("_")) + "Scenario"
    if not hasattr(module, class_name):
        print(
            f"Error: Class '{class_name}' not found in {module_name}", file=sys.stderr
        )
        raise SystemExit(1)

    return getattr(module, class_name)


def run_scenario(
    scenario_name: str,
    action: str,
    share_path: Path,
    source_vm: str | None = None,
) -> int:
    """Run a scenario action (write or verify)."""
    hostname = get_hostname()
    ctx = TestContext(
        role=hostname,
        scenario=scenario_name,
        path=share_path,
        source_vm=source_vm,
    )

    scenario_class = get_scenario(scenario_name)
    scenario = scenario_class()

    if not hasattr(scenario, action):
        print(
            f"Error: Action '{action}' not defined for scenario '{scenario_name}'",
            file=sys.stderr,
        )
        raise SystemExit(1)

    method = getattr(scenario, action)

    print(f"Running {scenario_name} [{hostname}] {action}...")
    try:
        method(ctx)
        print(f"Action '{action}' completed successfully")

        if ctx._metrics and ctx.metrics.metrics:
            print()
            print(ctx.metrics.report())

        return 0
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def cleanup_test_files(base_path: Path, dry_run: bool = False) -> int:
    """Remove all test files from basePath.

    Searches share/, export/, and quarantine/ for files with TEST_FILE_PREFIX.

    Args:
        base_path: Channel basePath (e.g., /storagevm/channel)
        dry_run: If True, only print what would be removed

    Returns:
        Number of items removed (or would be removed if dry_run)
    """
    if not base_path.exists():
        print(f"Error: Path does not exist: {base_path}", file=sys.stderr)
        return 0

    # Directories to search
    search_dirs = ["share", "export", "quarantine"]
    removed = 0

    for dir_name in search_dirs:
        search_path = base_path / dir_name
        if not search_path.exists():
            continue

        # Find all files/dirs with test prefix
        for item in search_path.rglob(f"{TEST_FILE_PREFIX}*"):
            if dry_run:
                print(f"  [dry-run] Would remove: {item}")
            else:
                try:
                    if item.is_symlink():
                        item.unlink()
                    elif item.is_dir():
                        shutil.rmtree(item)
                    else:
                        item.unlink()
                except FileNotFoundError:
                    # Already deleted (parent dir removed, or race condition)
                    pass
                except OSError as e:
                    print(f"  Error removing {item}: {e}", file=sys.stderr)
                    continue
            removed += 1

    if removed > 0:
        action = "Would remove" if dry_run else "Cleaned up"
        print(f"{action} {removed} test file(s)")

    return removed


def list_scenarios() -> None:
    """List available scenarios."""
    scenarios_dir = Path(__file__).parent / "scenarios"
    print("Available scenarios:")

    # List scenarios in category folders
    for category_dir in sorted(scenarios_dir.iterdir()):
        if not category_dir.is_dir() or category_dir.name.startswith("_"):
            continue

        category = category_dir.name
        scenarios = []
        for path in sorted(category_dir.glob("*.py")):
            if path.name.startswith("_"):
                continue
            scenarios.append(path.stem)

        if scenarios:
            print(f"\n  {category}/")
            for name in scenarios:
                print(f"    {category}.{name}")


def main() -> None:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        prog="ghaf-virtiofs-test",
        description="Integration tests for ghaf-virtiofs-tools",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # 'run' command
    run_parser = subparsers.add_parser("run", help="Run a test scenario")
    run_parser.add_argument(
        "share",
        type=Path,
        help="Path to share mount",
    )
    run_parser.add_argument(
        "--scenario",
        "-s",
        required=True,
        help="Scenario name (e.g., basic.rw, extended.large_file)",
    )
    action_group = run_parser.add_mutually_exclusive_group(required=True)
    action_group.add_argument(
        "--write",
        action="store_true",
        help="Run as writer (create test files)",
    )
    action_group.add_argument(
        "--verify",
        action="store_true",
        help="Run as verifier (check files appeared)",
    )
    run_parser.add_argument(
        "--source-vm",
        help="Source VM name (for wo tests to identify diode producer)",
    )

    # 'list' command
    subparsers.add_parser("list", help="List available scenarios")

    # 'clean' command
    clean_parser = subparsers.add_parser(
        "clean", help="Remove test files from basePath"
    )
    clean_parser.add_argument(
        "base_path",
        type=Path,
        help="Channel basePath (e.g., /storagevm/channel)",
    )
    clean_parser.add_argument(
        "--dry-run",
        "-n",
        action="store_true",
        help="Show what would be removed without removing",
    )

    args = parser.parse_args()

    if args.command == "list":
        list_scenarios()
    elif args.command == "clean":
        cleanup_test_files(args.base_path, dry_run=args.dry_run)
    elif args.command == "run":
        action = "write" if args.write else "verify"
        sys.exit(
            run_scenario(
                scenario_name=args.scenario,
                action=action,
                share_path=args.share,
                source_vm=args.source_vm,
            )
        )


if __name__ == "__main__":
    main()
