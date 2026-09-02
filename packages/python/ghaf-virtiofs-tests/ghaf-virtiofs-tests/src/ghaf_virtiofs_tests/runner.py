# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Test runner for coordinated VM and host test execution.

Uses a JSON config file to define channels (share types) with their paths.

Config file (JSON):
    {
      "channels": {
        "rw": {
          "host_path": "/persist/storagevm/shared/directories/gui-chrome-share",
          "vm": "gui-vm",
          "vm_path": "/Shares/Unsafe-chrome"
        },
        "ro": {
          "host_path": "/persist/storagevm/shared/directories/ghaf-identity",
          "vm": "gui-vm",
          "vm_path": "/etc/identity"
        },
        "wo": {
          "host_path": "/persist/storagevm/shared/directories/ghaf-keys",
          "vm": "gui-vm",
          "vm_path": "/etc/ghaf-keys"
        }
      },
      "vms": {
        "gui-vm": {
          "user": "ghaf",
          "password": "ghaf"
        }
      }
    }

Usage:
    # Run all tests
    ghaf-virtiofs-test-runner -c config.json

    # Run basic tests only
    ghaf-virtiofs-test-runner -c config.json --basic

    # Run specific category
    ghaf-virtiofs-test-runner -c config.json --category extended

    # Run specific test
    ghaf-virtiofs-test-runner -c config.json --test basic.rw

    # Host verification only
    ghaf-virtiofs-test-runner -c config.json --host-only
"""

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Literal


@dataclass
class TestSpec:
    """Test specification."""

    scenario: str
    share_type: Literal["rw", "ro", "wo", "host_only", "vm_only"]


@dataclass
class ChannelConfig:
    """Channel (share type) configuration."""

    host_path: str
    vm_name: str
    vm_path: str


@dataclass
class VMConfig:
    """VM SSH configuration."""

    name: str
    user: str
    password: str | None = None


@dataclass
class Config:
    """Runner configuration."""

    channels: dict[str, ChannelConfig]
    vms: dict[str, VMConfig]


# All test specifications
TESTS: list[TestSpec] = [
    # rw share tests
    TestSpec("basic.rw", "rw"),
    TestSpec("basic.modify", "rw"),
    TestSpec("basic.delete", "rw"),
    TestSpec("basic.rename", "rw"),
    TestSpec("basic.scan", "rw"),
    TestSpec("basic.performance", "rw"),
    TestSpec("extended.paths", "rw"),
    TestSpec("extended.permissions", "rw"),
    TestSpec("extended.symlink", "rw"),
    TestSpec("extended.ignore", "rw"),
    TestSpec("extended.large_file", "rw"),
    TestSpec("extended.quarantine", "rw"),
    TestSpec("extended.rename_rapid", "rw"),
    TestSpec("security.bypass", "rw"),
    TestSpec("security.overload", "rw"),
    TestSpec("vsock.scan", "rw"),
    TestSpec("vsock.proxy", "vm_only"),
    TestSpec("vsock.performance", "vm_only"),
    TestSpec("vsock.security", "vm_only"),
    # ro share tests
    TestSpec("basic.ro", "ro"),
    # wo share tests
    TestSpec("basic.wo", "wo"),
    # host-only tests
    TestSpec("extended.scan_overhead", "host_only"),
]

BASIC_TESTS = [t for t in TESTS if t.scenario.startswith("basic.")]

CATEGORIES = {
    "basic": [t for t in TESTS if t.scenario.startswith("basic.")],
    "extended": [t for t in TESTS if t.scenario.startswith("extended.")],
    "vsock": [t for t in TESTS if t.scenario.startswith("vsock.")],
    "security": [t for t in TESTS if t.scenario.startswith("security.")],
}


def load_config(path: Path) -> Config:
    """Load configuration from JSON file."""
    content = path.read_text()
    data = json.loads(content)

    # Load channels
    channels: dict[str, ChannelConfig] = {}
    for share_type, channel_data in data.get("channels", {}).items():
        channels[share_type] = ChannelConfig(
            host_path=channel_data["host_path"],
            vm_name=channel_data["vm"],
            vm_path=channel_data["vm_path"],
        )

    # Load VMs
    vms: dict[str, VMConfig] = {}
    for name, vm_data in data.get("vms", {}).items():
        vms[name] = VMConfig(
            name=name,
            user=vm_data.get("user", "ghaf"),
            password=vm_data.get("password"),
        )

    return Config(channels=channels, vms=vms)


def run_ssh_cmd(vm: VMConfig, cmd: list[str], timeout: int = 300) -> bool:
    """Run command on VM via SSH with sudo."""
    ssh_target = f"{vm.user}@{vm.name}"
    env = None

    # Wrap command with sudo (use password if available)
    if vm.password:
        # Use echo to pipe password to sudo -S
        escaped_cmd = " ".join(cmd)
        remote_cmd = f"echo '{vm.password}' | sudo -S {escaped_cmd}"
        ssh_cmd_args = [remote_cmd]
    else:
        # No password - assume passwordless sudo or already root
        ssh_cmd_args = ["sudo"] + cmd

    if vm.password:
        # Use sshpass with environment variable to hide password from ps
        env = {**os.environ, "SSHPASS": vm.password}
        full_cmd = [
            "sshpass",
            "-e",  # Read password from SSHPASS env var
            "ssh",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            f"ConnectTimeout={timeout}",
            ssh_target,
        ] + ssh_cmd_args
    else:
        # Use SSH key auth
        full_cmd = [
            "ssh",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            f"ConnectTimeout={timeout}",
            ssh_target,
        ] + ssh_cmd_args

    print(f"    $ ssh {ssh_target} sudo {' '.join(cmd)}")
    try:
        result = subprocess.run(
            full_cmd, capture_output=False, timeout=timeout, env=env
        )
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        print(f"    SSH command timed out after {timeout}s")
        return False


def run_local_cmd(cmd: list[str]) -> bool:
    """Run local command."""
    print(f"    $ {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=False)
    return result.returncode == 0


def run_test(spec: TestSpec, config: Config) -> bool | None:
    """Run a single test with VM and host coordination.

    Returns:
        True if passed, False if failed, None if skipped.
    """
    print(f"\n{'=' * 60}")
    print(f"TEST: {spec.scenario} [{spec.share_type}]")
    print(f"{'=' * 60}")

    # Host-only test - use rw channel's host_path
    if spec.share_type == "host_only":
        if "rw" not in config.channels:
            print("  SKIP: No rw channel configured for host-only test")
            return None
        host_path = config.channels["rw"].host_path
        print("  [host verify]")
        return run_local_cmd(
            ["ghaf-virtiofs-test", "run", host_path, "-s", spec.scenario, "--verify"]
        )

    # VM-only test (no share needed, runs in /tmp)
    if spec.share_type == "vm_only":
        if not config.vms:
            print("  SKIP: No VMs configured")
            return None
        # Use first available VM
        vm = next(iter(config.vms.values()))
        print(f"  [vm write - {vm.name}]")
        vm_cmd = ["ghaf-virtiofs-test", "run", "/tmp", "-s", spec.scenario, "--write"]
        return run_ssh_cmd(vm, vm_cmd)

    # Channel-based test (rw, ro, wo)
    if spec.share_type not in config.channels:
        print(f"  SKIP: No {spec.share_type} channel configured")
        return None

    channel = config.channels[spec.share_type]

    if channel.vm_name not in config.vms:
        print(f"  SKIP: VM '{channel.vm_name}' not found in vms config")
        return None

    vm = config.vms[channel.vm_name]
    vm_cmd = [
        "ghaf-virtiofs-test",
        "run",
        channel.vm_path,
        "-s",
        spec.scenario,
        "--write",
    ]
    host_cmd = [
        "ghaf-virtiofs-test",
        "run",
        channel.host_path,
        "-s",
        spec.scenario,
        "--verify",
    ]
    # Pass source VM name for tests that need it (e.g., wo diode tests)
    if spec.share_type == "wo":
        host_cmd.extend(["--source-vm", channel.vm_name])

    # Always start host verify first (in background), then run VM write
    print("  [host verify - background]")
    host_proc = subprocess.Popen(
        host_cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )

    print(f"  [vm write - {vm.name}]")
    vm_ok = run_ssh_cmd(vm, vm_cmd)

    print("  [host verify - waiting]")
    host_out, _ = host_proc.communicate(timeout=300)
    host_ok = host_proc.returncode == 0

    if host_out:
        for line in host_out.decode().splitlines():
            print(f"    {line}")

    return vm_ok and host_ok


def run_host_only(spec: TestSpec, config: Config) -> bool | None:
    """Run host verification only."""
    print(f"\n{'=' * 60}")
    print(f"VERIFY: {spec.scenario}")
    print(f"{'=' * 60}")

    # Determine host path based on share type
    if spec.share_type in ("host_only", "vm_only"):
        # Use rw channel for these
        if "rw" not in config.channels:
            print("  SKIP: No rw channel configured")
            return None
        host_path = config.channels["rw"].host_path
    elif spec.share_type in config.channels:
        host_path = config.channels[spec.share_type].host_path
    else:
        print(f"  SKIP: No {spec.share_type} channel configured")
        return None

    return run_local_cmd(
        ["ghaf-virtiofs-test", "run", host_path, "-s", spec.scenario, "--verify"]
    )


def print_summary(results: dict[str, bool | None]) -> int:
    """Print summary and return exit code."""
    print(f"\n{'=' * 60}")
    print("SUMMARY")
    print(f"{'=' * 60}")

    passed = 0
    failed = 0
    skipped = 0

    for name, ok in results.items():
        if ok is None:
            status = "SKIP"
            skipped += 1
        elif ok:
            status = "PASS"
            passed += 1
        else:
            status = "FAIL"
            failed += 1
        print(f"  {name}: {status}")

    print(f"\n{passed} passed, {failed} failed, {skipped} skipped")
    return 0 if failed == 0 else 1


def main() -> None:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        prog="ghaf-virtiofs-test-runner",
        description="Coordinated test runner with config-based VM management",
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        help="Path to JSON config file (required for running tests)",
    )
    parser.add_argument(
        "--basic",
        action="store_true",
        help="Run basic tests only",
    )
    parser.add_argument(
        "--category",
        choices=list(CATEGORIES.keys()),
        help="Run specific category",
    )
    parser.add_argument(
        "--test",
        help="Run specific test (e.g., basic.rw)",
    )
    parser.add_argument(
        "--host-only",
        action="store_true",
        help="Run host verification only (no VM tests)",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List all tests and exit",
    )

    args = parser.parse_args()

    if args.list:
        print("Available tests:\n")
        for share_type in ["rw", "ro", "wo", "vm_only", "host_only"]:
            tests = [t for t in TESTS if t.share_type == share_type]
            if tests:
                print(f"  [{share_type}]")
                for t in tests:
                    print(f"    {t.scenario}")
                print()
        return

    # Config required for running tests
    if not args.config:
        print("Error: --config required for running tests")
        sys.exit(1)

    if not args.config.exists():
        print(f"Error: Config file not found: {args.config}")
        sys.exit(1)

    config = load_config(args.config)
    print(f"Config loaded: {len(config.channels)} channels, {len(config.vms)} VMs")
    for share_type, channel in config.channels.items():
        print(
            f"  [{share_type}] {channel.vm_name}:{channel.vm_path} -> host:{channel.host_path}"
        )

    # Determine which tests to run
    if args.test:
        tests = [t for t in TESTS if t.scenario == args.test]
        if not tests:
            print(f"Error: Unknown test '{args.test}'")
            sys.exit(1)
    elif args.basic:
        tests = BASIC_TESTS
    elif args.category:
        tests = CATEGORIES[args.category]
    else:
        tests = TESTS

    # Run tests
    results: dict[str, bool | None] = {}

    for spec in tests:
        if args.host_only:
            results[spec.scenario] = run_host_only(spec, config)
        else:
            results[spec.scenario] = run_test(spec, config)

    sys.exit(print_summary(results))


if __name__ == "__main__":
    main()
