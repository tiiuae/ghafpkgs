<!--
SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# Watcher Module

Recursive inotify-based file watcher with debouncing to trigger file scans on modification.

The watcher monitors directory trees for file changes and emits events after a
configurable debounce period. It is designed for scenarios where files may be
written multiple times in rapid succession (e.g., editors saving with temp files,
downloads in progress) and you only want to process the final state.

## Features

- **Recursive watching**: Automatically watches subdirectories as they are created
- **Debouncing**: Coalesces rapid modifications into a single event
- **Move tracking**: Detects renames within watched trees vs moves in/out
- **Loop prevention**: Skips events for files the handler just wrote
- **Overflow recovery**: Handling of inotify queue overflow
- **Memory bounded**: Triggers recovery when queues grow too large
- **Source tagging**: Events include the source identifier (e.g., which VM)

## Design Summary

### What the watcher handles

- CLOSE_WRITE: file content finalized after write
- DELETE: file removed
- MOVED_FROM/MOVED_TO: file renamed (paired via cookie)
- CREATE (directories): extends watch tree

### What the watcher ignores

- MODIFY: intermediate writes before close
- ATTRIB: metadata-only changes
- ACCESS, OPEN, CLOSE_NOWRITE: read operations

### Assumptions

1. **inotify coverage**: The watcher relies on CLOSE_WRITE to detect modifications,
   assuming programs close files after writing. Edge cases (mmap, truncate, process
   crashes, virtiofs quirks) may not generate events.

2. **Filesystem permissions**: VMs can only write to their own share directories.
   Loop prevention uses FileId (device, inode) which differs between reflinked copies.

3. **Event ordering**: inotify delivers events in order per watch descriptor.
   Cross-directory ordering is not guaranteed but handled via cookie timeouts.

### Known limitations

- **No formal verification**: The transition table and state cleanup are manually reviewed,
  not machine-checked.

- **Overflow loses events**: During inotify overflow or memory pressure, modifications
  may be missed. External resync (or new write) required to restore consistency.

- **Race window**: Between inotify event and metadata stat, file state may change.
  Failures are logged and skipped.

- **Symlinks rejected**: O_NOFOLLOW prevents symlink traversal; symlinked files are not processed.

### Design Goals

The implementation targets these properties (states are implicit in HashMaps):

1. **Modified only after debounce**: Content should be stable before scan
2. **Renamed only for idle files**: Already scanned or pre-existing
3. **Pending write+rename emits Deleted+Modified**: Forces rescan at new location
4. **All paths reach idle**: Timeouts should prevent stuck entries
5. **Deterministic**: Each event follows one code path

Every entry in internal state has a guaranteed exit path:

| Collection | Entry Added | Entry Removed |
| ------------ | ------------- | --------------- |
| `pending` | CW, MT_NEW | DEL, MF, DB_EXP, MT (with pending) |
| `pending_by_path` | CW, MT_NEW | DEL, MF, DB_EXP, MT (mirrors pending) |
| `pending_moves` | MF | MT (cookie match), CK_EXP (timeout) |
| `ready` | Any output | Consumer calls `next()` |
| `skip_cache` | Handler return | LRU eviction |

All PM_* states transition to IDLE via either MT or CK_EXP (timeout).
Overflow recovery clears all state as a failsafe.

## Configuration

| Field | Type | Default | Description |
| ------- | ------ | --------- | ------------- |
| `debounce_duration` | Duration | 300ms | Wait after last write before emit |
| `move_cookie_timeout` | Duration | 2s | Window to match MF with MT |
| `excludes` | Vec | empty | Directory paths to skip |

### Internal Constants

| Constant | Value | Description |
| ---------- | ------- | ------------- |
| `DEFAULT_MAX_PENDING` | 16,384 | Force-flush oldest when exceeded |
| `MAX_TOTAL_QUEUED` | 65,536 | Trigger overflow recovery |
| `SKIP_CACHE_SIZE` | 16,384 | LRU cache for loop prevention |

## Usage

```rust
use ghaf_virtiofs_tools::watcher::{EventHandler, FileId, Watcher, WatcherConfig};
use std::path::Path;
use std::time::Duration;

struct MyHandler;

impl EventHandler for MyHandler {
    fn on_modified(&mut self, path: &Path, source: &str) -> Vec<(FileId, i64)> {
        println!("Modified: {} from {}", path.display(), source);
        vec![] // Return (FileId, ctime) for loop prevention
    }

    fn on_deleted(&mut self, path: &Path, source: &str) {
        println!("Deleted: {} from {}", path.display(), source);
    }

    fn on_renamed(&mut self, path: &Path, old_path: &Path, source: &str) -> Vec<(FileId, i64)> {
        println!("Renamed: {} -> {}", old_path.display(), path.display());
        vec![]
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = WatcherConfig {
        debounce_duration: Duration::from_millis(500),
        ..Default::default()
    };

    let mut watcher = Watcher::with_config(config)?;
    watcher.add_recursive(Path::new("/watched/path"), "my-source")?;

    let mut handler = MyHandler;
    watcher.run(&mut handler).await;
    Ok(())
}
```

## Architecture

### Module Structure

```text
watcher/
  config.rs     WatcherConfig struct
  constants.rs  Timing and size constants
  core.rs       Watcher struct and main event loop
  event.rs      FileEvent, FileEventKind, EventHandler trait
  overflow.rs   Overflow detection and recovery
  pending.rs    Debounce state and timeout management
  tests.rs      Integration tests (see TEST_PLAN.md)
```

### Event Flow

```text
Kernel inotify
      |
      v
+----------------------+
| handle_inotify_event |
+----------------------+
      |
      +--> CREATE (dir) ---------> add_recursive (watch new subtree)
      |
      +--> DELETE ---------------> Deleted (immediate)
      |
      +--> MOVED_FROM -----------> pending_moves (store cookie)
      |
      +--> MOVED_TO + cookie ----> Renamed or Deleted + pending
      |
      +--> MOVED_TO no cookie ---> pending (debounce)
      |
      +--> CLOSE_WRITE ----------> pending (debounce)
                                        |
                                  [debounce timeout]
                                        |
                                        v
                                   Modified
```

### Overflow Recovery

When inotify queue overflows or internal queues exceed limits:

1. Clear all internal state
2. Wait with exponential backoff (2s to 60s)
3. Re-establish watches

Files modified during overflow are lost. Restart daemon for consistency.

### Loop Prevention

Handler returns `(FileId, ctime)` pairs for written files. Watcher skips
future events for files whose ctime **exactly matches** the cached value.

- **No time window**: Skip only if `current_ctime == cached_ctime` (exact equality)
- **Any modification invalidates**: If another process touches the file, ctime changes
- **LRU bounded**: Cache holds 16,384 entries; oldest evicted automatically

### States

| State | Description |
| ------- | ------------- |
| IDLE | No pending write |
| PENDING | Write received, debounce timer running |
| PM_IDLE | MOVED_FROM received, file was IDLE |
| PM_PENDING | MOVED_FROM received, file was PENDING |

#### Input Events

| Event | Source | Description |
| ------- | -------- | ------------- |
| CW | inotify | CLOSE_WRITE - file closed after write |
| DEL | inotify | DELETE - file removed |
| MF | inotify | MOVED_FROM - rename source (emits cookie) |
| MT | inotify | MOVED_TO with cookie match |
| MT_NEW | inotify | MOVED_TO without cookie (move-in) |
| DB_EXP | timer | Debounce timer expires |
| CK_EXP | timer | Cookie timer expires |

#### Output Events

| Output | Handler Method | Description |
| -------- | ---------------- | ------------- |
| Modified | `on_modified(path)` | File needs scanning (after debounce) |
| Deleted | `on_deleted(path)` | File removed |
| Renamed | `on_renamed(new, old)` | File moved, no scan needed |

#### Transition Table (28 transitions)

| State | Event | Next State | Output | Notes |
| ------- | ------- | ------------ | -------- | ------- |
| IDLE | CW | PENDING | - | Start debounce timer |
| IDLE | DEL | IDLE | Deleted | Immediate |
| IDLE | MF | PM_IDLE | - | Store cookie, remember was idle |
| IDLE | MT | IDLE | - | No-op (no pending move) |
| IDLE | MT_NEW | PENDING | - | Move-in, start debounce |
| IDLE | DB_EXP | IDLE | - | No-op (no pending) |
| IDLE | CK_EXP | IDLE | - | No-op (no pending move) |
| PENDING | CW | PENDING | - | Reset debounce timer |
| PENDING | DEL | IDLE | Deleted | Cancel pending |
| PENDING | MF | PM_PENDING | - | Store cookie, remember was pending |
| PENDING | MT | PENDING | - | No-op (no pending move) |
| PENDING | MT_NEW | PENDING | - | Reset debounce timer |
| PENDING | DB_EXP | IDLE | Modified | Debounce complete |
| PENDING | CK_EXP | PENDING | - | No-op (no pending move) |
| PM_IDLE | CW | PM_IDLE | - | Ignored while awaiting MT |
| PM_IDLE | DEL | PM_IDLE | - | Ignored while awaiting MT |
| PM_IDLE | MF | PM_IDLE | - | Ignored while awaiting MT |
| PM_IDLE | MT | IDLE | Renamed | Cookie matched, file was idle |
| PM_IDLE | MT_NEW | PM_IDLE | - | Ignored while awaiting MT |
| PM_IDLE | DB_EXP | PM_IDLE | - | Ignored while awaiting MT |
| PM_IDLE | CK_EXP | IDLE | Deleted | Move-out (no MT arrived) |
| PM_PENDING | CW | PM_PENDING | - | Ignored while awaiting MT |
| PM_PENDING | DEL | PM_PENDING | - | Ignored while awaiting MT |
| PM_PENDING | MF | PM_PENDING | - | Ignored while awaiting MT |
| PM_PENDING | MT | PENDING | Deleted | Force rescan at new path |
| PM_PENDING | MT_NEW | PM_PENDING | - | Ignored while awaiting MT |
| PM_PENDING | DB_EXP | PM_PENDING | - | Ignored while awaiting MT |
| PM_PENDING | CK_EXP | IDLE | Deleted | Pending file moved out |

The transition table documents 4 states x 7 events = 28 transitions.
This table is manually maintained and must be updated when transition logic changes.

#### Output-Producing Transitions (7 total)

| Transition | Output | Description |
| ------------ | -------- | ------------- |
| IDLE + DEL | Deleted | Immediate |
| PENDING + DEL | Deleted | Cancel pending, immediate |
| PENDING + DB_EXP | Modified | Debounce complete |
| PM_IDLE + MT | Renamed | File was already scanned |
| PM_IDLE + CK_EXP | Deleted | Move-out timeout |
| PM_PENDING + MT | Deleted | Force rescan at new location |
| PM_PENDING + CK_EXP | Deleted | Pending file moved out |

## Test Plan

See [TEST_PLAN.md](TEST_PLAN.md) for test case definitions.
