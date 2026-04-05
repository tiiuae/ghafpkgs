// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for watcher event handling.
//!
//! Test IDs follow the convention in `TEST_PLAN.md`:
//! - `W-OUT-xx`: Output-producing transitions (7 core behaviors)
//! - `W-SEQ-xx`: Multi-step sequences (edge cases)
//! - `W-PAT-xx`: Real-world patterns (editor saves, rotations)
//! - `W-STR-xx`: Stress tests (overflow, high load)

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;
    use std::time::Duration;

    use tempfile::TempDir;
    use tokio::time::timeout;

    use crate::watcher::{FileEvent, FileEventKind, Watcher, WatcherConfig};

    /// Debounce duration for tests. 100ms provides enough margin for CI jitter.
    const TEST_DEBOUNCE: Duration = Duration::from_millis(100);

    /// Move cookie timeout for tests.
    const TEST_COOKIE_TIMEOUT: Duration = Duration::from_millis(300);

    /// Timeout for waiting for events. Must accommodate debounce + cookie + CI jitter.
    const EVENT_TIMEOUT: Duration = Duration::from_millis(2000);

    /// Sleep duration to stay within debounce window (30% of `TEST_DEBOUNCE`).
    const WITHIN_DEBOUNCE: Duration = Duration::from_millis(30);

    /// Create a watcher with test configuration.
    fn test_watcher() -> (Watcher, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        let config = WatcherConfig {
            debounce_duration: TEST_DEBOUNCE,
            move_cookie_timeout: TEST_COOKIE_TIMEOUT,
            excludes: vec![],
        };
        let mut watcher = Watcher::with_config(config).expect("create watcher");
        watcher
            .add_recursive(dir.path(), "test")
            .expect("add watch");
        (watcher, dir)
    }

    /// Wait for next event with timeout.
    async fn next_event(watcher: &mut Watcher) -> Option<FileEvent> {
        timeout(EVENT_TIMEOUT, watcher.next()).await.ok().flatten()
    }

    /// Wait for specific event kind, discarding others.
    /// NOTE: This is lossy - use `collect_events_for()` when you need to verify exact sequences.
    async fn wait_for_kind(watcher: &mut Watcher, kind: &str) -> Option<FileEvent> {
        for _ in 0..10 {
            if let Some(event) = next_event(watcher).await {
                let kind_matches = matches!(
                    (&event.kind, kind),
                    (FileEventKind::Modified, "Modified")
                        | (FileEventKind::Deleted, "Deleted")
                        | (FileEventKind::Renamed { .. }, "Renamed")
                );
                if kind_matches {
                    return Some(event);
                }
            } else {
                break;
            }
        }
        None
    }

    /// Collect all events within a time window. Use this for exact sequence verification.
    async fn collect_events_for(watcher: &mut Watcher, duration: Duration) -> Vec<FileEvent> {
        let mut events = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < duration {
            let remaining = duration.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, watcher.next()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) | Err(_) => break,
            }
        }
        events
    }

    /// Assert event sequence contains expected kinds in order (may have other events between).
    fn assert_sequence_contains(events: &[FileEvent], expected: &[&str]) {
        let mut expected_iter = expected.iter();
        let mut current_expected = expected_iter.next();

        for event in events {
            if let Some(exp) = current_expected {
                let matches = matches!(
                    (&event.kind, *exp),
                    (FileEventKind::Modified, "Modified")
                        | (FileEventKind::Deleted, "Deleted")
                        | (FileEventKind::Renamed { .. }, "Renamed")
                );
                if matches {
                    current_expected = expected_iter.next();
                }
            }
        }

        assert!(
            current_expected.is_none(),
            "sequence {expected:?} not found in events: {events:?}",
        );
    }

    /// Get event kind as string for comparison.
    fn event_kind_str(event: &FileEvent) -> &'static str {
        match &event.kind {
            FileEventKind::Modified => "Modified",
            FileEventKind::Deleted => "Deleted",
            FileEventKind::Renamed { .. } => "Renamed",
        }
    }

    /// Filter events to specific path and return kinds as strings.
    fn events_for_path(events: &[FileEvent], path: &std::path::Path) -> Vec<&'static str> {
        events
            .iter()
            .filter(|e| e.path == path)
            .map(event_kind_str)
            .collect()
    }

    /// Assert exact event sequence for a specific path (no spurious events allowed).
    fn assert_exact_events(events: &[FileEvent], path: &std::path::Path, expected: &[&str]) {
        let actual = events_for_path(events, path);
        assert_eq!(
            actual, expected,
            "exact events for {path:?} - expected {expected:?}, got {actual:?}\nall events: {events:?}",
        );
    }

    // =========================================================================
    // Output-Producing Transitions (W-OUT-xx)
    // =========================================================================

    /// `W-OUT-03`: `CW`, `DB_EXP` -> Modified (`PENDING` + `DB_EXP` -> `IDLE`)
    #[tokio::test]
    async fn test_close_write_clean() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Write and close file
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"hello").unwrap();
        }

        // Should get Modified after debounce
        let event = wait_for_kind(&mut watcher, "Modified").await;
        assert!(event.is_some(), "expected Modified event");
        assert_eq!(event.unwrap().path, path);
    }

    /// `W-SEQ-01`: `CW`, `CW`, `DB_EXP` -> Modified (debounce coalesces)
    #[tokio::test]
    async fn test_close_write_pending_coalesce() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Write multiple times quickly (within debounce window)
        for i in 0..3 {
            let mut f = File::create(&path).unwrap();
            f.write_all(format!("content {i}").as_bytes()).unwrap();
            tokio::time::sleep(WITHIN_DEBOUNCE).await;
        }

        // Collect all events - should coalesce to exactly one Modified
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;
        assert_exact_events(&events, &path, &["Modified"]);
    }

    /// `W-OUT-01`: `DEL` -> Deleted (IDLE + DEL -> IDLE)
    #[tokio::test]
    async fn test_delete_clean() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Create file and wait for it to reach IDLE state (Modified emitted)
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"content").unwrap();
        }
        let modified = wait_for_kind(&mut watcher, "Modified").await;
        assert!(modified.is_some(), "file should reach IDLE via Modified");

        // Now delete from IDLE state
        fs::remove_file(&path).unwrap();

        // Should get Deleted
        let event = wait_for_kind(&mut watcher, "Deleted").await;
        assert!(event.is_some(), "expected Deleted event");
        assert_eq!(event.unwrap().path, path);
    }

    /// `W-OUT-02`: `CW`, `DEL` -> Deleted (PENDING + DEL -> IDLE)
    #[tokio::test]
    async fn test_delete_pending() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Write file
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"hello").unwrap();
        }

        // Delete before debounce expires
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::remove_file(&path).unwrap();

        // Should get Deleted only, no Modified
        let event = wait_for_kind(&mut watcher, "Deleted").await;
        assert!(event.is_some(), "expected Deleted event");

        // Wait past debounce time - no Modified should come
        tokio::time::sleep(TEST_DEBOUNCE * 2).await;
        let extra = next_event(&mut watcher).await;
        assert!(
            extra.is_none(),
            "expected no Modified after delete, got {extra:?}"
        );
    }

    // =========================================================================
    // Move Tests (W-OUT-04 to W-OUT-07)
    // =========================================================================

    /// `W-OUT-04`: `MF`, `MT` -> Renamed (`PM_IDLE` + `MT` -> `IDLE`)
    #[tokio::test]
    async fn test_rename_same_inode() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Create file first, wait for it to be processed
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"content").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Rename
        fs::rename(&old_path, &new_path).unwrap();

        // Should get Renamed event
        let event = wait_for_kind(&mut watcher, "Renamed").await;
        assert!(event.is_some(), "expected Renamed event");
        let event = event.unwrap();
        assert_eq!(event.path, new_path);
        if let FileEventKind::Renamed { old_path: op } = event.kind {
            assert_eq!(op, old_path);
        } else {
            panic!("expected Renamed kind");
        }
    }

    /// `W-OUT-07`: `CW`, `MF`, `CK_EXP` -> Deleted (`PM_PENDING` + `CK_EXP` -> `IDLE`)
    #[tokio::test]
    async fn test_write_then_move_out() {
        let (mut watcher, dir) = test_watcher();
        let watched_path = dir.path().join("file.txt");

        // Create another temp dir outside watched tree
        let outside_dir = TempDir::new().unwrap();
        let outside_path = outside_dir.path().join("file.txt");

        // Write file
        {
            let mut f = File::create(&watched_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Move out of watched tree before debounce
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&watched_path, &outside_path).unwrap();

        // Wait for cookie timeout + debounce to ensure all events are collected
        let events =
            collect_events_for(&mut watcher, TEST_COOKIE_TIMEOUT + TEST_DEBOUNCE * 2).await;

        // Should get exactly Deleted, no Modified (move-out cancels pending)
        assert_exact_events(&events, &watched_path, &["Deleted"]);
    }

    /// `W-OUT-06`: `CW`, `MF`, `MT` -> Deleted (`PM_PENDING` + `MT` -> `PENDING`)
    #[tokio::test]
    async fn test_write_then_rename() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Write file
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Rename before debounce
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&old_path, &new_path).unwrap();

        // Collect events
        let mut events = Vec::new();
        for _ in 0..5 {
            if let Some(e) = next_event(&mut watcher).await {
                events.push(e);
            }
        }

        // Pending file renamed: should emit Deleted(old) + Modified(new)
        // (NOT Renamed, because file was never scanned - needs scan at new location)
        let has_deleted_old = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Deleted) && e.path == old_path);

        let has_modified_new = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == new_path);

        // Should NOT have Modified at old_path (file was renamed before scan)
        let has_modified_old = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == old_path);

        assert!(has_deleted_old, "expected Deleted at old path: {events:?}");
        assert!(
            has_modified_new,
            "expected Modified at new path: {events:?}"
        );
        assert!(
            !has_modified_old,
            "BUG: Modified at old path after rename: {events:?}"
        );
    }

    /// `W-OUT-03` variant: `MT_NEW`, `DB_EXP` -> Modified (move in from outside)
    #[tokio::test]
    async fn test_move_in_from_outside() {
        let (mut watcher, dir) = test_watcher();

        // Create file outside watched tree
        let outside_dir = TempDir::new().unwrap();
        let outside_path = outside_dir.path().join("file.txt");
        {
            let mut f = File::create(&outside_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        let watched_path = dir.path().join("file.txt");

        // Move into watched tree
        fs::rename(&outside_path, &watched_path).unwrap();

        // Should get Modified (treated as new file)
        let event = wait_for_kind(&mut watcher, "Modified").await;
        assert!(event.is_some(), "expected Modified event");
        assert_eq!(event.unwrap().path, watched_path);
    }

    // =========================================================================
    // Multi-Step Sequences (W-SEQ-xx)
    // =========================================================================

    /// `W-SEQ-07`: `MF`, `MT`, `CW`, `DB_EXP` -> Renamed, Modified
    #[tokio::test]
    async fn test_rename_then_write() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Create file, wait for IDLE state
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"original").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Rename (IDLE -> Renamed)
        fs::rename(&old_path, &new_path).unwrap();

        // Write to renamed file (new PENDING)
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        {
            let mut f = fs::OpenOptions::new().write(true).open(&new_path).unwrap();
            f.write_all(b"updated").unwrap();
        }

        // Collect all events and verify sequence
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify Renamed comes before Modified
        assert_sequence_contains(&events, &["Renamed", "Modified"]);

        // Verify correct paths
        let renamed = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Renamed { .. }));
        let modified = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Modified));

        assert!(renamed.is_some(), "expected Renamed event: {events:?}");
        assert_eq!(renamed.unwrap().path, new_path);

        assert!(modified.is_some(), "expected Modified event: {events:?}");
        assert_eq!(modified.unwrap().path, new_path);
    }

    /// `W-SEQ-08`: `MF`, `MT`, `DEL` -> Renamed, Deleted
    #[tokio::test]
    async fn test_rename_then_delete() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Create file, wait for IDLE state
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"content").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Rename (IDLE -> Renamed)
        fs::rename(&old_path, &new_path).unwrap();

        // Delete
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::remove_file(&new_path).unwrap();

        // Collect all events and verify sequence
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify Renamed comes before Deleted
        assert_sequence_contains(&events, &["Renamed", "Deleted"]);

        // Verify correct paths
        let renamed = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Renamed { .. }));
        let deleted = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Deleted) && e.path == new_path);

        assert!(renamed.is_some(), "expected Renamed event: {events:?}");
        assert!(deleted.is_some(), "expected Deleted(new_path): {events:?}");
    }

    /// `W-SEQ-09`: `CW`, `MF`, `MT`, `CW`, `DB_EXP` -> Deleted, Modified
    #[tokio::test]
    async fn test_pending_rename_then_write() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Write (PENDING)
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"original").unwrap();
        }

        // Rename while PENDING
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&old_path, &new_path).unwrap();

        // Write again to new path
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        {
            let mut f = fs::OpenOptions::new().write(true).open(&new_path).unwrap();
            f.write_all(b"updated").unwrap();
        }

        // Collect all events
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify sequence: Deleted(old) comes before Modified(new)
        assert_sequence_contains(&events, &["Deleted", "Modified"]);

        // Verify correct paths and no spurious events
        let deleted_old = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Deleted) && e.path == old_path);
        let modified_new = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Modified) && e.path == new_path);
        let modified_old = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == old_path);

        assert!(deleted_old.is_some(), "expected Deleted(old): {events:?}");
        assert!(modified_new.is_some(), "expected Modified(new): {events:?}");
        assert!(!modified_old, "BUG: Modified(old) after rename: {events:?}");
    }

    /// `W-OUT-05`: `MF`, `CK_EXP` -> Deleted (`PM_IDLE` + `CK_EXP` -> `IDLE`)
    #[tokio::test]
    async fn test_move_out_idle() {
        let (mut watcher, dir) = test_watcher();
        let watched_path = dir.path().join("file.txt");
        let outside_dir = TempDir::new().unwrap();
        let outside_path = outside_dir.path().join("file.txt");

        // Create file, wait for IDLE
        {
            let mut f = File::create(&watched_path).unwrap();
            f.write_all(b"content").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Move out of watched tree
        fs::rename(&watched_path, &outside_path).unwrap();

        // Should get Deleted after cookie timeout
        let event = wait_for_kind(&mut watcher, "Deleted").await;
        assert!(event.is_some(), "expected Deleted event");
        assert_eq!(event.unwrap().path, watched_path);
    }

    /// `W-SEQ-03`: `DEL`, `CW`, `DB_EXP` -> Deleted, Modified (recreate)
    #[tokio::test]
    async fn test_delete_then_recreate() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Create initial file
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"v1").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Delete
        fs::remove_file(&path).unwrap();
        let event = wait_for_kind(&mut watcher, "Deleted").await;
        assert!(event.is_some(), "expected Deleted");

        // Recreate
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"v2").unwrap();
        }
        let event = wait_for_kind(&mut watcher, "Modified").await;
        assert!(event.is_some(), "expected Modified for recreated file");
    }

    /// `W-SEQ-04`: `CW`, `MF`, `MT`, `DEL` -> Deleted, Deleted
    #[tokio::test]
    async fn test_write_rename_delete() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Write (PENDING)
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Rename while PENDING
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&old_path, &new_path).unwrap();

        // Delete
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::remove_file(&new_path).unwrap();

        // Collect all events
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify exact events per path
        // old_path: exactly Deleted (from pending rename)
        assert_exact_events(&events, &old_path, &["Deleted"]);

        // new_path: exactly Deleted (delete cancels any pending)
        assert_exact_events(&events, &new_path, &["Deleted"]);
    }

    /// `W-SEQ-05`: `MT_NEW`, `DEL` -> Deleted (move-in then immediate delete)
    #[tokio::test]
    async fn test_move_in_then_delete() {
        let (mut watcher, dir) = test_watcher();

        // Create file outside watched tree
        let outside_dir = TempDir::new().unwrap();
        let outside_path = outside_dir.path().join("file.txt");
        {
            let mut f = File::create(&outside_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        let watched_path = dir.path().join("file.txt");

        // Move into watched tree
        fs::rename(&outside_path, &watched_path).unwrap();

        // Delete immediately (before debounce)
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::remove_file(&watched_path).unwrap();

        // Collect all events - should get exactly Deleted, no Modified
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;
        assert_exact_events(&events, &watched_path, &["Deleted"]);
    }

    /// `W-SEQ-06`: `CW`, `MF`, `MT`, `DB_EXP` -> Deleted, Modified
    #[tokio::test]
    async fn test_pending_rename_then_debounce() {
        let (mut watcher, dir) = test_watcher();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        // Write (PENDING)
        {
            let mut f = File::create(&old_path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Rename while PENDING
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&old_path, &new_path).unwrap();

        // Collect all events (includes waiting for debounce)
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify sequence: Deleted(old) comes before Modified(new)
        assert_sequence_contains(&events, &["Deleted", "Modified"]);

        // Verify correct paths and no spurious events
        let deleted_old = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Deleted) && e.path == old_path);
        let modified_new = events
            .iter()
            .find(|e| matches!(e.kind, FileEventKind::Modified) && e.path == new_path);
        let modified_old = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == old_path);

        assert!(deleted_old.is_some(), "expected Deleted(old): {events:?}");
        assert!(modified_new.is_some(), "expected Modified(new): {events:?}");
        assert!(!modified_old, "BUG: Modified(old) after rename: {events:?}");
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    /// W-PAT-08: Rapid create-delete cycles (no orphan pending entries)
    #[tokio::test]
    async fn test_rapid_create_delete_cycles() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        for _ in 0..5 {
            // Create
            {
                let mut f = File::create(&path).unwrap();
                f.write_all(b"x").unwrap();
            }
            // Delete immediately
            fs::remove_file(&path).unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Wait for all events to settle
        tokio::time::sleep(TEST_DEBOUNCE * 3).await;

        // Drain events
        let mut events = Vec::new();
        while let Some(e) = next_event(&mut watcher).await {
            events.push(e);
        }

        // Should only have Delete events, no orphan Modified
        let modified_count = events
            .iter()
            .filter(|e| matches!(e.kind, FileEventKind::Modified))
            .count();

        assert_eq!(
            modified_count, 0,
            "expected no Modified events after delete, got {modified_count}: {events:?}"
        );
    }

    /// Test that source is correctly propagated for all event types
    #[tokio::test]
    async fn test_source_propagation() {
        let dir = TempDir::new().unwrap();
        let config = WatcherConfig {
            debounce_duration: TEST_DEBOUNCE,
            move_cookie_timeout: TEST_COOKIE_TIMEOUT,
            excludes: vec![],
        };
        let mut watcher = Watcher::with_config(config).unwrap();
        watcher.add_recursive(dir.path(), "my-source").unwrap();

        let path = dir.path().join("file.txt");
        let new_path = dir.path().join("renamed.txt");

        // Test Modified source
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"hello").unwrap();
        }
        let modified = wait_for_kind(&mut watcher, "Modified").await;
        assert!(modified.is_some(), "expected Modified event");
        assert_eq!(
            &*modified.unwrap().source,
            "my-source",
            "Modified source mismatch"
        );

        // Test Renamed source
        fs::rename(&path, &new_path).unwrap();
        let renamed = wait_for_kind(&mut watcher, "Renamed").await;
        assert!(renamed.is_some(), "expected Renamed event");
        assert_eq!(
            &*renamed.unwrap().source,
            "my-source",
            "Renamed source mismatch"
        );

        // Test Deleted source
        fs::remove_file(&new_path).unwrap();
        let deleted = wait_for_kind(&mut watcher, "Deleted").await;
        assert!(deleted.is_some(), "expected Deleted event");
        assert_eq!(
            &*deleted.unwrap().source,
            "my-source",
            "Deleted source mismatch"
        );
    }

    // =========================================================================
    // Real-World Patterns (W-PAT-xx)
    // =========================================================================

    /// W-PAT-01: Atomic save (write temp, rename to target)
    #[tokio::test]
    async fn test_atomic_save_pattern() {
        let (mut watcher, dir) = test_watcher();
        let target = dir.path().join("document.txt");
        let temp = dir.path().join(".document.txt.tmp");

        // Initial file - wait for IDLE state
        {
            let mut f = File::create(&target).unwrap();
            f.write_all(b"original").unwrap();
        }
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Atomic save: write temp, rename over target
        {
            let mut f = File::create(&temp).unwrap();
            f.write_all(b"updated").unwrap();
        }
        fs::rename(&temp, &target).unwrap();

        // Wait for debounce and collect all events
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Key assertion: Modified(target) must occur (user sees updated content)
        let has_modified_target = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == target);

        // Negative assertion: no orphan Modified(temp) - it was renamed, not scanned at temp path
        let has_modified_temp = events
            .iter()
            .any(|e| matches!(e.kind, FileEventKind::Modified) && e.path == temp);

        assert!(
            has_modified_target,
            "atomic save must emit Modified(target): {events:?}"
        );
        assert!(
            !has_modified_temp,
            "temp file should not emit Modified (was renamed): {events:?}"
        );
    }

    /// W-PAT-02 / W-SEQ-02: Chain renames (backup rotation)
    #[tokio::test]
    async fn test_backup_rotation() {
        let (mut watcher, dir) = test_watcher();
        let file = dir.path().join("file.txt");
        let bak = dir.path().join("file.txt.bak");
        let old = dir.path().join("file.txt.old");

        // Create initial files and wait for IDLE state
        File::create(&file).unwrap().write_all(b"current").unwrap();
        File::create(&bak).unwrap().write_all(b"backup").unwrap();

        // Wait for both files to reach IDLE
        let _ = wait_for_kind(&mut watcher, "Modified").await;
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Rotation: bak -> old, file -> bak
        fs::rename(&bak, &old).unwrap();
        fs::rename(&file, &bak).unwrap();

        // Collect all events
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Verify specific rename pairs
        let has_bak_to_old = events.iter().any(|e| {
            if let FileEventKind::Renamed { old_path } = &e.kind {
                e.path == old && *old_path == bak
            } else {
                false
            }
        });

        let has_file_to_bak = events.iter().any(|e| {
            if let FileEventKind::Renamed { old_path } = &e.kind {
                e.path == bak && *old_path == file
            } else {
                false
            }
        });

        assert!(has_bak_to_old, "expected Renamed(bak -> old): {events:?}");
        assert!(has_file_to_bak, "expected Renamed(file -> bak): {events:?}");
    }

    /// W-PAT-03: Replace via move (move replacement over existing)
    /// Note: Kernel behavior varies for rename-over-existing, so we accept
    /// either Renamed or Modified for the target path.
    #[tokio::test]
    async fn test_replace_via_move() {
        let (mut watcher, dir) = test_watcher();
        let target = dir.path().join("target.txt");
        let replacement = dir.path().join("replacement.txt");

        // Create both files and wait for IDLE
        File::create(&target).unwrap().write_all(b"old").unwrap();
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        File::create(&replacement)
            .unwrap()
            .write_all(b"new")
            .unwrap();
        let _ = wait_for_kind(&mut watcher, "Modified").await;

        // Replace target with replacement
        fs::rename(&replacement, &target).unwrap();

        // Collect events
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        // Accept either Renamed or Modified for target - kernel behavior varies
        let target_updated = events.iter().any(|e| {
            e.path == target
                && matches!(
                    e.kind,
                    FileEventKind::Renamed { .. } | FileEventKind::Modified
                )
        });

        // replacement should not have orphan Modified (it was renamed away)
        let replacement_modified = events
            .iter()
            .any(|e| e.path == replacement && matches!(e.kind, FileEventKind::Modified));

        assert!(
            target_updated,
            "expected Renamed or Modified for target: {events:?}"
        );
        assert!(
            !replacement_modified,
            "replacement should not emit Modified (was renamed): {events:?}"
        );
    }

    /// W-PAT-04: Nested directory rename (no panic on watch invalidation)
    #[tokio::test]
    async fn test_nested_dir_rename_no_panic() {
        let (mut watcher, dir) = test_watcher();
        let old_dir = dir.path().join("old_dir");
        let new_dir = dir.path().join("new_dir");
        fs::create_dir(&old_dir).unwrap();

        // Give watcher time to add watch for new directory
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Trigger event processing
        let _ = tokio::time::timeout(Duration::from_millis(50), watcher.next()).await;

        let file_in_old = old_dir.join("file.txt");

        // Write to file in old_dir
        {
            let mut f = File::create(&file_in_old).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Rename directory before debounce
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        fs::rename(&old_dir, &new_dir).unwrap();

        // Drain events - primary goal is no panic
        for _ in 0..10 {
            let _ = next_event(&mut watcher).await;
        }

        // Test passes if we reach here without panic
    }

    /// W-PAT-05: Multiple files modified concurrently
    #[tokio::test]
    async fn test_concurrent_modifications() {
        let (mut watcher, dir) = test_watcher();

        // Create multiple files rapidly
        for i in 0..5 {
            let path = dir.path().join(format!("file{i}.txt"));
            let mut f = File::create(&path).unwrap();
            f.write_all(format!("content{i}").as_bytes()).unwrap();
        }

        // Collect events
        let mut modified_count = 0;
        for _ in 0..15 {
            if let Some(e) = next_event(&mut watcher).await {
                if matches!(e.kind, FileEventKind::Modified) {
                    modified_count += 1;
                }
            }
        }

        assert_eq!(
            modified_count, 5,
            "expected 5 Modified events, got {modified_count}"
        );
    }

    /// W-PAT-06: Write, truncate to zero, write again
    #[tokio::test]
    async fn test_truncate_and_rewrite() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Initial write
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"initial content").unwrap();
        }

        // Truncate and rewrite within debounce window
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        {
            let mut f = File::create(&path).unwrap(); // truncates
            f.write_all(b"new").unwrap();
        }

        // Collect all events - should coalesce to exactly one Modified
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;
        assert_exact_events(&events, &path, &["Modified"]);
    }

    /// W-PAT-07: Empty file creation then write
    #[tokio::test]
    async fn test_create_empty_then_write() {
        let (mut watcher, dir) = test_watcher();
        let path = dir.path().join("file.txt");

        // Create empty
        File::create(&path).unwrap();

        // Write content within debounce window
        tokio::time::sleep(WITHIN_DEBOUNCE).await;
        {
            let mut f = fs::OpenOptions::new().write(true).open(&path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Collect all events - should coalesce to exactly one Modified
        let events = collect_events_for(&mut watcher, TEST_DEBOUNCE * 3).await;

        let modified_count = events
            .iter()
            .filter(|e| matches!(e.kind, FileEventKind::Modified) && e.path == path)
            .count();

        assert_eq!(
            modified_count, 1,
            "expected exactly 1 Modified (coalesced), got {modified_count}: {events:?}"
        );
    }

    // =========================================================================
    // Stress Tests (W-STR-xx) - run with: cargo test -- --ignored
    // =========================================================================

    /// W-STR-01: Many files created rapidly
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_many_files() {
        let (mut watcher, dir) = test_watcher();
        let file_count = 1000;

        // Create many files rapidly
        for i in 0..file_count {
            let path = dir.path().join(format!("file{i:04}.txt"));
            let mut f = File::create(&path).unwrap();
            f.write_all(b"x").unwrap();
        }

        // Collect all events
        let mut modified_count = 0;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(30) {
            if let Some(e) = next_event(&mut watcher).await {
                if matches!(e.kind, FileEventKind::Modified) {
                    modified_count += 1;
                }
                if modified_count >= file_count {
                    break;
                }
            }
        }

        assert_eq!(
            modified_count, file_count,
            "expected {file_count} Modified events, got {modified_count}"
        );
    }

    /// W-STR-02: Rapid write/delete cycles (no orphan Modified events)
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_rapid_cycles() {
        let (mut watcher, dir) = test_watcher();
        let cycle_count = 500;
        let path = dir.path().join("file.txt");

        // Rapid create-delete cycles: each create is immediately deleted
        for _ in 0..cycle_count {
            {
                let mut f = File::create(&path).unwrap();
                f.write_all(b"x").unwrap();
            }
            fs::remove_file(&path).unwrap();
        }

        // Wait for all events to settle
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Drain all events
        let mut events = Vec::new();
        while let Ok(Some(e)) =
            tokio::time::timeout(Duration::from_millis(100), watcher.next()).await
        {
            events.push(e);
        }

        // Key invariant: no Modified events should exist for this path
        // because each create is immediately followed by delete, which cancels pending
        let orphan_modified: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, FileEventKind::Modified) && e.path == path)
            .collect();

        assert!(
            orphan_modified.is_empty(),
            "found {} orphan Modified events (delete should cancel pending): {:?}",
            orphan_modified.len(),
            events
        );
    }

    /// W-STR-03: Deeply nested directory structure
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_deep_nesting() {
        let (mut watcher, dir) = test_watcher();
        let depth = 50;

        // Create deeply nested structure, draining events between each level
        // to allow watcher to add watches on new directories
        let mut current = dir.path().to_path_buf();
        for i in 0..depth {
            current = current.join(format!("level{i}"));
            fs::create_dir(&current).unwrap();

            // Let watcher process directory creation and add watch
            tokio::time::sleep(WITHIN_DEBOUNCE).await;
            let _ = tokio::time::timeout(Duration::from_millis(50), watcher.next()).await;
        }

        // Extra time for final watches to settle
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Write file at deepest level
        let deep_file = current.join("deep.txt");
        {
            let mut f = File::create(&deep_file).unwrap();
            f.write_all(b"deep content").unwrap();
        }

        // Should still receive event
        let event = wait_for_kind(&mut watcher, "Modified").await;
        assert!(event.is_some(), "expected Modified event for deep file");
        assert_eq!(event.unwrap().path, deep_file);
    }

    /// W-STR-04: Sustained high rate (verify no file starvation under load)
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_sustained_rate() {
        use std::collections::HashSet;

        let (mut watcher, dir) = test_watcher();
        let duration = Duration::from_secs(5);
        let interval = Duration::from_millis(10);
        let unique_files: usize = 50;

        let start = std::time::Instant::now();
        let mut write_count: usize = 0;

        // Sustained writes to a rotating set of files
        // With 50 files at 10ms intervals, each file revisited every 500ms
        // Debounce is 100ms, so multiple events per file are expected
        while start.elapsed() < duration {
            let path = dir
                .path()
                .join(format!("file{}.txt", write_count % unique_files));
            {
                let mut f = File::create(&path).unwrap();
                f.write_all(format!("{write_count}").as_bytes()).unwrap();
            }
            write_count += 1;
            tokio::time::sleep(interval).await;
        }

        // Wait for final debounces to complete
        tokio::time::sleep(TEST_DEBOUNCE * 2).await;

        // Drain all events and track which files were seen
        let mut seen_files: HashSet<std::path::PathBuf> = HashSet::new();
        let mut event_count: usize = 0;
        while let Ok(Some(event)) =
            tokio::time::timeout(Duration::from_millis(500), watcher.next()).await
        {
            if matches!(event.kind, FileEventKind::Modified) {
                seen_files.insert(event.path);
            }
            event_count += 1;
        }

        // Key invariant: every file got at least one Modified event (no starvation)
        assert_eq!(
            seen_files.len(),
            unique_files,
            "expected all {unique_files} files to receive events, got {} (total events: {event_count})",
            seen_files.len()
        );
    }

    /// W-STR-05: Slow consumer (simulated scanning delay)
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_slow_consumer() {
        let (mut watcher, dir) = test_watcher();
        let file_count = 50;
        let scan_delay = Duration::from_millis(100); // Simulate 100ms scan per file

        // Create files rapidly (producer is fast)
        for i in 0..file_count {
            let path = dir.path().join(format!("file{i:02}.txt"));
            let mut f = File::create(&path).unwrap();
            f.write_all(b"content").unwrap();
        }

        // Process events slowly (consumer is slow)
        let mut processed = 0;
        let start = std::time::Instant::now();

        while start.elapsed() < Duration::from_secs(30) {
            if let Ok(Some(event)) =
                tokio::time::timeout(Duration::from_millis(500), watcher.next()).await
            {
                if matches!(event.kind, FileEventKind::Modified) {
                    // Simulate slow scanning
                    tokio::time::sleep(scan_delay).await;
                    processed += 1;

                    if processed >= file_count {
                        break;
                    }
                }
            } else {
                break;
            }
        }

        assert_eq!(
            processed, file_count,
            "expected {file_count} events processed despite slow consumer, got {processed}"
        );
    }

    /// W-STR-06: Slow consumer with concurrent writes
    #[tokio::test]
    #[ignore = "stress test - run manually with --ignored"]
    async fn test_overload_concurrent_slow_consumer() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (mut watcher, dir) = test_watcher();
        let write_count = Arc::new(AtomicUsize::new(0));
        let write_count_clone = write_count.clone();
        let dir_path = dir.path().to_path_buf();

        // Producer task: write files continuously for 5 seconds
        let producer = tokio::spawn(async move {
            let duration = Duration::from_secs(5);
            let start = std::time::Instant::now();

            while start.elapsed() < duration {
                let count = write_count_clone.fetch_add(1, Ordering::SeqCst);
                let path = dir_path.join(format!("file{}.txt", count % 20));
                {
                    let mut f = File::create(&path).unwrap();
                    f.write_all(format!("{count}").as_bytes()).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        // Consumer: process events with simulated 200ms scan delay
        let mut processed = 0;
        let scan_delay = Duration::from_millis(200);
        let consumer_start = std::time::Instant::now();

        while consumer_start.elapsed() < Duration::from_secs(30) {
            if let Ok(Some(event)) =
                tokio::time::timeout(Duration::from_millis(1000), watcher.next()).await
            {
                if matches!(event.kind, FileEventKind::Modified) {
                    tokio::time::sleep(scan_delay).await;
                    processed += 1;
                }
            } else if producer.is_finished() {
                // Producer done and no more events
                break;
            }
        }

        let total_writes = write_count.load(Ordering::SeqCst);

        // Should process significant portion despite slow consumer
        // Due to debouncing, may have fewer events than writes
        assert!(
            processed > 0,
            "expected some events processed, got {processed} from {total_writes} writes"
        );

        // Ensure producer finished
        let _ = producer.await;
    }
}
