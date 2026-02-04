# virtiofs-gate

Host daemon to secure cross-VM file sharing over virtiofs with virus scanning.

## Overview

The virtiofs-gate daemon monitors shared directories for file changes, scans files with ClamAV, and propagates clean files to other VMs. Infected files are quarantined or deleted based on configuration.

## How It Works

### File Flow

1. Producer VM writes file to `share/{producer}/`
2. inotify detects `IN_CLOSE_WRITE` event
3. File is cloned to `staging/` via FICLONE (atomic snapshot)
4. Staged file is scanned with ClamAV
5. If clean: reflink to other producers' shares and `export/`
6. If infected: quarantine, delete, or log based on config
7. Guest VMs are notified of changes via vsock
8. If infected: user notification written to socket

### Channels

A channel groups VMs that share files:
- **Producers**: VMs with read-write access (bidirectional sync)
- **Consumers**: VMs with read-only access (via `export/`)

Each channel operates independently with its own configuration. The daemon runs all channels concurrently using async Tokio tasks, allowing parallel processing of file events across channels.

### Loop Prevention

When virtiofs-gate writes files to producer shares, it tracks the (device, inode, ctime) of written files in an LRU cache. When inotify reports events for these files, they are skipped to avoid infinite re-processing of reflinks.

### Startup Sync

On daemon startup, each channel performs a sync phase before starting the watcher:

1. **Inventory**: Scan all producer directories and export to build file lists with mtime/size
2. **Diff**: Compare inventories to detect inconsistencies:
   - Files in some producers but not others
   - Files in producers but missing from export
   - Conflicting versions across producers (different mtime/size)
   - Orphan files in export (not in any producer)
3. **Resolve**: For conflicts, the file with the latest mtime wins
4. **Execute**: Trigger handler for files needing sync, delete orphans from export

This ensures channel consistency after:
- Daemon restart with files modified while offline
- VM crashes that left partial state
- Manual file manipulation on the host

The sync runs before the watcher starts, so no inotify events are generated for sync operations.

## Extensibility

The daemon uses trait-based abstractions for key components:

- **Scanner** (`VirusScanner` trait): Currently implements ClamAV via clamd socket. Can be extended to support other scanners by implementing `validate_availability()`, `scan_path()`, and `scan_fd()`.

- **Watcher** (`Watcher`): inotify-based file system watcher with debouncing. Handles `IN_CLOSE_WRITE`, `IN_DELETE`, and `IN_MOVED_TO` events. Can be swapped with a different watcher implementation (e.g., fanotify-based).

- **Notifier**: Sends channel notifications to guest VMs over vsock to trigger file browser refresh. For compatibility with all file browsers it uses a toggled trigger file, as some file browsers do not refresh the file tree upon touches or metadata updates, and implement debouncing.

## Failure Handling

| Failure | Behavior |
|---------|----------|
| ClamAV unavailable at startup | Warning logged, daemon continues |
| Scan error (permissive=true) | File treated as clean, propagated |
| Scan error (permissive=false) | File not propagated |
| File disappeared before scan | Skipped silently (common with temp files) |
| Reflink to target fails | Logged, continues with other targets |
| Quarantine fails | Falls back to delete, logs warning |
| Guest notification fails | Logged as warning, non-blocking |
| Channel config invalid | Channel skipped, other channels continue |

When ClamAV is unavailable, the daemon logs a warning and continues. Permissive channels will propagate files (treating scan errors as clean), while non-permissive channels will skip files until ClamAV becomes available.

## Directory Structure

Each channel requires this host directory layout:

```
{basePath}/
  staging/           # temporary files during scan
  share/
    {producer1}/     # virtiofs rw mount for producer1
    {producer2}/     # virtiofs rw mount for producer2
  export/            # scanned files for consumers
  quarantine/        # infected files (if quarantine enabled)
```

The `export/` directory can be bind-mounted read-only as `export-ro/` for consumer VMs. This is handled externally by the system configuration, not by the daemon itself.

## Configuration

JSON configuration file with channel definitions:

```json
{
  "documents": {
    "basePath": "/var/lib/virtiofs/documents",
    "producers": ["chromium-vm", "office-vm"],
    "consumers": ["reader-vm"],
    "debounceMs": 1000,
    "scanning": {
      "infectedAction": "quarantine",
      "permissive": false,
      "ignoreFilePatterns": [".crdownload", ".part", "~$"],
      "ignorePathPatterns": [".Trash-"]
    },
    "notify": {
      "guests": [10, 11],
      "port": 3401
    }
  }
}
```

### Channel Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `basePath` | string | required | Root directory for channel |
| `producers` | array | required | VM names with rw access |
| `consumers` | array | `[]` | VM names with ro access |
| `debounceMs` | number | `1000` | Wait time after last write |

The `debounceMs` option controls how long the daemon waits after detecting a file write before processing it. This allows large file copies or multi-write operations to complete before scanning. Lower values provide faster response but may scan incomplete files; higher values are safer for large transfers but add latency.

### Scanning Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `infectedAction` | string | `"delete"` | `log`, `delete`, or `quarantine` |
| `permissive` | bool | `false` | Treat scan errors as clean |
| `ignoreFilePatterns` | array | `[]` | Filename patterns to skip |
| `ignorePathPatterns` | array | `[]` | Path patterns to skip |
| `notifySocket` | string | `/run/clamav/notify.sock` | User notification socket |

Ignore patterns prevent unnecessary scanning and syncing:
- **File patterns** match against filenames only. Use for temporary files that are still being written (e.g., `.crdownload`, `.part`, `~$` for browser downloads and Office temp files).
- **Path patterns** match against the full relative path. Use for system directories that should not be synced (e.g., `.Trash-`, `.local/share/Trash`).

### Notify Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `guests` | array | `[]` | Guest VM CIDs to notify |
| `port` | number | `3401` | vsock port for notifications |

## CLI Usage

```bash
# Start daemon with configuration
virtiofs-gate run --config /etc/virtiofs/channels.json

# Start with debug logging
virtiofs-gate run --config /etc/virtiofs/channels.json --debug

# Verify configuration without starting
virtiofs-gate verify --config /etc/virtiofs/channels.json --verbose
```

## Requirements

- **Root**: Must run as root (virtiofs passthrough requires host UID mapping)
- **Filesystem**: btrfs or XFS with reflink support (FICLONE)
- **Capability**: `CAP_CHOWN` for preserving file ownership
- **ClamAV**: clamd running with accessible socket
- **vsock**: For guest notifications (optional)
