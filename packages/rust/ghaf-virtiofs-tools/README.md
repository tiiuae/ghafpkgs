<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# ghaf-virtiofs-tools

Tools for secure cross-VM file sharing over virtiofs with integrated virus scanning.

## Components

| Binary | Location | Description |
| ------ | -------- | ----------- |
| `virtiofs-gate` | Host | Gateway daemon - scans files and propagates to VMs |
| `clamd-vclient` | Guest | On-write scanner - watches directories, scans via vsock |
| `clamd-vproxy` | Host | ClamAV proxy - filters commands, forwards to clamd |
| `virtiofs-notify` | Guest | Notification receiver - triggers file browser refresh |

## Use Cases

### Shared Directories with Scanning (virtiofs-gate)

Secure file sharing between VMs with automatic virus scanning.

```text
  ┌─────────────────────────────────────┐
  │            GUEST VM A               │
  │                                     │
  │   App writes file.txt               │
  └──────────────┬──────────────────────┘
                 │ virtiofs
                 ▼
  ┌─────────────────────────────────────┐
  │              HOST                   │
  │                                     │
  │   share/vm-a/file.txt               │
  │         │                           │
  │         │ clone to staging          │
  │         ▼                           │
  │   staging/file.txt ───► clamd       │
  │         │                           │
  │         │ clean?                    │
  │         ▼                           │
  │   ┌─────┴─────┐                     │
  │   ▼           ▼                     │
  │ share/vm-b/ export/                 │
  │ file.txt    file.txt                │
  │ (reflink)   (reflink)               │
  │                                     │
  └──────────────┬──────────────────────┘
                 │ virtiofs
                 ▼
  ┌─────────────────────────────────────┐
  │            GUEST VM B               │
  │                                     │
  │   file.txt appears                  │
  └─────────────────────────────────────┘
```

Infected files are quarantined or deleted instead of propagated.

See [src/gate/README.md](src/gate/README.md)

### On-Write Scanning (clamd-vclient + clamd-vproxy)

Scan files written inside a guest VM using host's ClamAV.

```text
  ┌─────────────────────────────────────┐
  │            GUEST VM                 │
  │                                     │
  │   App writes file.txt               │
  │         │                           │
  │         ▼                           │
  │   ~/Downloads/file.txt              │
  │         │                           │
  │   clamd-vclient detects write       │
  │         │                           │
  │   read file contents                │
  │         │                           │
  └─────────┼───────────────────────────┘
            │ vsock
            ▼
  ┌─────────────────────────────────────┐
  │              HOST                   │
  │                                     │
  │   clamd-vproxy receives stream      │
  │         │                           │
  │         │ filter: INSTREAM only     │
  │         ▼                           │
  │   clamd scans stream                │
  │         │                           │
  │   returns: clean / infected         │
  │         │                           │
  └─────────┼───────────────────────────┘
            │ vsock
            ▼
  ┌─────────────────────────────────────┐
  │            GUEST VM                 │
  │                                     │
  │   clamd-vclient receives result     │
  │         │                           │
  │         ▼                           │
  │   infected? ───► delete/quarantine  │
  │                                     │
  └─────────────────────────────────────┘
```

### File Browser Refresh (virtiofs-notify)

Notify guests when new files appear in shared directories.

```text
  ┌─────────────────────────────────────┐
  │              HOST                   │
  │                                     │
  │   virtiofs-gate exports file        │
  │   to share/vm-b/                    │
  │         │                           │
  │   send "channel-name"               │
  │         │                           │
  └─────────┼───────────────────────────┘
            │ vsock
            ▼
  ┌─────────────────────────────────────┐
  │            GUEST VM                 │
  │                                     │
  │   virtiofs-notify receives          │
  │   "channel-name"                    │
  │         │                           │
  │   lookup: channel ───► /mnt/share   │
  │         │                           │
  │   toggle .virtiofs-refresh          │
  │         │                           │
  │         ▼                           │
  │   inotify fires ───► file browser   │
  │                      refreshes      │
  │                                     │
  └─────────────────────────────────────┘
```

## Building

```bash
# Build with nix
nix build .#ghaf-virtiofs-tools

# Build with cargo
cargo build --release
```

## Requirements

- **Filesystem**: btrfs or XFS with reflink support (for virtiofs-gate)
- **ClamAV**: clamd daemon for virus scanning
- **vsock**: vhost-vsock for guest communication
- **virtiofs**: For shared directory mounts

## License

Apache-2.0
