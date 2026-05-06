// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Public event types for the file watcher.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Unique identifier for a file (device + inode).
pub type FileId = (u64, u64);

/// File event emitted by the watcher.
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Full path to the file.
    pub path: PathBuf,
    /// Source identifier (e.g., producer name). Empty if not set.
    pub source: Arc<str>,
    /// Event type.
    pub kind: FileEventKind,
}

/// Type of file event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEventKind {
    /// File was modified (closed after write, or moved in from outside).
    Modified,
    /// File was deleted (or moved out of watched tree).
    Deleted,
    /// File was renamed within watched tree (same inode, different path).
    Renamed { old_path: PathBuf },
}

/// Trait for handling file events from the watcher.
///
/// Implement this trait to define custom behavior for file events.
/// The watcher will call the appropriate method for each event.
pub trait EventHandler {
    /// Called when a file is modified (after debounce period).
    ///
    /// Returns (`FileId`, ctime) pairs to mark as written (for loop prevention).
    /// Events are skipped only if the file's ctime still matches.
    fn on_modified(&mut self, path: &Path, source: &str) -> Vec<(FileId, i64)>;

    /// Called when a file is deleted.
    fn on_deleted(&mut self, path: &Path, source: &str);

    /// Called when a file is renamed (same inode, different path).
    /// No scanning needed - content unchanged.
    ///
    /// Returns (`FileId`, ctime) pairs to mark as written (for loop prevention).
    fn on_renamed(&mut self, path: &Path, old_path: &Path, source: &str) -> Vec<(FileId, i64)>;
}
