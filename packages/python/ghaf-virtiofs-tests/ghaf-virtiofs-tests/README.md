<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# ghaf-virtiofs-tests

Integration tests for ghaf-virtiofs-tools on live Ghaf systems. Tests are split
into different categories, with `basic` tests expected to cover rudimentary
functionality.

Note that this test suite is simplistically designed - it does not use elaborate
sync mechanisms to check results between guest and host, and does not include
guest-to-guest tests. For the latter, it is assumed sufficient that files follow
the propagation expectations which is checked in the host folders.

Each channel maps a share type (rw, ro, wo) to:

- `host_path`: Path on host where gate runs
- `vm`: VM name (must match entry in vms section)
- `vm_path`: Path inside VM where share is mounted

**WARNING**: The test runner config contains cleartext passwords and is intended
for test environments only. Do not use in production or commit config files with
real credentials to version control.

## Share Types

- **rw**: Read-write producer share - can create, modify, delete files
- **ro**: Read-only consumer export - can only read files
- **wo**: Write-only diode share - can write but not read from other producers

## Test Categories

- **basic**: Core file sync operations (create, modify, delete, rename, diode)
- **extended**: Edge cases (paths, permissions, symlinks, large files)
- **security**: Bypass attempts and DoS resilience
- **vsock**: vclient on-write scanning and ClamAV proxy via vsock

See TEST_PLAN.md for detailed test specifications.

## Usage

### Single Test

```bash
# On VM (write test files)
ghaf-virtiofs-test run /mnt/share -s basic.rw --write

# On host (verify files synced)
ghaf-virtiofs-test run /persist/channel -s basic.rw --verify

# List available scenarios
ghaf-virtiofs-test list
```

### Test Runner (Automated)

The test runner coordinates VM and host tests automatically via SSH.

```bash
# Run all tests
ghaf-virtiofs-test-runner -c config.json

# Run basic tests only
ghaf-virtiofs-test-runner -c config.json --basic

# Run specific category
ghaf-virtiofs-test-runner -c config.json --category extended

# Run specific test
ghaf-virtiofs-test-runner -c config.json --test basic.rw

# List available tests
ghaf-virtiofs-test-runner --list
```

Config file format (JSON):

```json
{
  "channels": {
    "rw": {
      "host_path": "/persist/gui-chrome-share",
      "vm": "gui-vm",
      "vm_path": "/Shares/Unsafe-chrome"
    },
    "ro": {
      "host_path": "/persist/ghaf-identity",
      "vm": "gui-vm",
      "vm_path": "/etc/identity"
    },
    "wo": {
      "host_path": "/persist/ghaf-keys",
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
```
