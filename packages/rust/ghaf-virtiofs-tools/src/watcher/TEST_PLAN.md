<!--
SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# Watcher Test Plan

Integration tests for the watcher module. Tests verify end-to-end behavior from
file operations to watcher output events.

Note: These tests verify file operation sequences, not specific inotify event
sequences. The mapping from file operations to inotify events depends on
kernel and filesystem behavior.

## Test Labeling Convention

```
W-{category}-{number}: {description}
```

Categories:
- **OUT**: Output-producing file operations
- **SEQ**: Multi-step sequences
- **PAT**: Real-world patterns (editor saves, rotations)
- **STR**: Stress tests (overflow, high load)

## Output-Producing Operations

Tests for file operations that produce watcher output events.

| ID | File Operation | Expected Output |
|----|----------------|-----------------|
| W-OUT-01 | delete | Deleted |
| W-OUT-02 | write, delete | Deleted |
| W-OUT-03 | write, wait | Modified |
| W-OUT-04 | rename | Renamed |
| W-OUT-05 | rename out of tree, wait | Deleted |
| W-OUT-06 | write, rename | Deleted (+ Modified at new path) |
| W-OUT-07 | write, rename out of tree, wait | Deleted |

## Multi-Step Sequences

Tests for complex file operation sequences.

| ID | File Operations | Expected Output | Description |
|----|-----------------|-----------------|-------------|
| W-SEQ-01 | write, write, wait | Modified | Debounce coalescing |
| W-SEQ-02 | rename, rename | Renamed, Renamed | Chain renames |
| W-SEQ-03 | delete, write, wait | Deleted, Modified | File recreate |
| W-SEQ-04 | write, rename, delete | Deleted, Deleted | Pending rename then delete |
| W-SEQ-05 | move-in, delete | Deleted | Move-in then immediate delete |
| W-SEQ-06 | write, rename, wait | Deleted, Modified | Pending rename then debounce |
| W-SEQ-07 | rename, write, wait | Renamed, Modified | Rename then write |
| W-SEQ-08 | rename, delete | Renamed, Deleted | Rename then delete |
| W-SEQ-09 | write, rename, write, wait | Deleted, Modified | Pending rename, write again |

## Real-World Patterns

Tests for common application behaviors.

| ID | Pattern | Description |
|----|---------|-------------|
| W-PAT-01 | Atomic save | Write temp, rename to target (editors) |
| W-PAT-02 | Backup rotation | file -> file.bak -> file.old |
| W-PAT-03 | Replace via move | Move replacement over existing file |
| W-PAT-04 | Nested dir rename | Directory rename (watch invalidation) |
| W-PAT-05 | Concurrent writes | Multiple files modified simultaneously |
| W-PAT-06 | Truncate rewrite | Truncate to zero, write new content |
| W-PAT-07 | Create empty then write | Touch then write content |
| W-PAT-08 | Rapid create-delete | Fast cycles, no orphan pending |

## Stress Tests

Run with `cargo test -- --ignored`

| ID | Scenario | Description |
|----|----------|-------------|
| W-STR-01 | Many files | 1000 files created rapidly |
| W-STR-02 | Rapid cycles | 500 create-delete cycles |
| W-STR-03 | Deep nesting | 50-level directory tree |
| W-STR-04 | Sustained rate | 10s continuous writes |
| W-STR-05 | Slow consumer | Simulated scan delay |
| W-STR-06 | Concurrent slow | Producer + slow consumer |

## Test Implementation Mapping

| Test Function | ID |
|---------------|-----|
| test_close_write_clean | W-OUT-03 |
| test_close_write_pending_coalesce | W-SEQ-01 |
| test_delete_clean | W-OUT-01 |
| test_delete_pending | W-OUT-02 |
| test_rename_same_inode | W-OUT-04 |
| test_write_then_move_out | W-OUT-07 |
| test_write_then_rename | W-OUT-06 |
| test_move_in_from_outside | W-OUT-03 (move-in variant) |
| test_rename_then_write | W-SEQ-07 |
| test_rename_then_delete | W-SEQ-08 |
| test_pending_rename_then_write | W-SEQ-09 |
| test_move_out_idle | W-OUT-05 |
| test_delete_then_recreate | W-SEQ-03 |
| test_write_rename_delete | W-SEQ-04 |
| test_move_in_then_delete | W-SEQ-05 |
| test_pending_rename_then_debounce | W-SEQ-06 |
| test_rapid_create_delete_cycles | W-PAT-08 |
| test_source_propagation | (config test) |
| test_atomic_save_pattern | W-PAT-01 |
| test_backup_rotation | W-PAT-02 |
| test_replace_via_move | W-PAT-03 |
| test_nested_dir_rename_no_panic | W-PAT-04 |
| test_concurrent_modifications | W-PAT-05 |
| test_truncate_and_rewrite | W-PAT-06 |
| test_create_empty_then_write | W-PAT-07 |
| test_overload_many_files | W-STR-01 |
| test_overload_rapid_cycles | W-STR-02 |
| test_overload_deep_nesting | W-STR-03 |
| test_overload_sustained_rate | W-STR-04 |
| test_overload_slow_consumer | W-STR-05 |
| test_overload_concurrent_slow_consumer | W-STR-06 |

## Test Count

| Category | Count |
|----------|-------|
| OUT | 7 |
| SEQ | 9 |
| PAT | 8 |
| STR | 6 |
| Other | 1 |
| **Total** | **31** |
