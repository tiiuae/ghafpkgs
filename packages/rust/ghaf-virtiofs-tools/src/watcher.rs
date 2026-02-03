// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Inotify-based file watcher with debouncing.
//!
//! Provides a reusable watcher that monitors directories for file changes
//! and emits debounced events. Used by both host and guest daemons.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::num::NonZeroUsize;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures::StreamExt;
use inotify::{EventMask, EventStream, Inotify, WatchDescriptor, WatchMask};
use log::{debug, warn};
use lru::LruCache;

// =============================================================================
// Constants
// =============================================================================

/// Default debounce duration - wait this long after last write before emitting event.
pub const DEFAULT_DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

/// Inotify read buffer size.
const INOTIFY_BUFFER_SIZE: usize = 4096;

/// Minimum poll interval to avoid busy-looping.
const MIN_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Default maximum pending files before forcing early processing.
const DEFAULT_MAX_PENDING: usize = 10000;

/// Overflow backoff: base delay before rescan.
const OVERFLOW_BACKOFF_BASE: Duration = Duration::from_secs(2);

/// Overflow backoff: maximum delay (cap for exponential growth).
const OVERFLOW_BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Overflow backoff: reset count after this period without overflow.
const OVERFLOW_RESET_AFTER: Duration = Duration::from_secs(300);

// =============================================================================
// Types
// =============================================================================

/// Unique identifier for a file (device + inode).
pub type FileId = (u64, u64);

/// File event emitted by the watcher.
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Full path to the file.
    pub path: PathBuf,
    /// Source identifier (e.g., producer name). Empty if not set.
    pub source: String,
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

/// Configuration for the watcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Duration to wait after last write before emitting event.
    pub debounce_duration: Duration,
    /// Maximum number of pending files before forcing early processing.
    pub max_pending: usize,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_duration: DEFAULT_DEBOUNCE_DURATION,
            max_pending: DEFAULT_MAX_PENDING,
        }
    }
}

// =============================================================================
// Internal Types
// =============================================================================

/// Information about a watched directory.
struct WatchInfo {
    /// Source identifier (e.g., producer name).
    source: String,
    /// Actual directory being watched.
    dir_path: PathBuf,
}

/// A file pending event emission after debounce.
struct PendingEntry {
    path: PathBuf,
    source: String,
    deadline: Instant,
}

impl PendingEntry {
    fn new(path: PathBuf, source: String, debounce: Duration) -> Self {
        Self {
            path,
            source,
            deadline: Instant::now() + debounce,
        }
    }

    fn reset_deadline(&mut self, debounce: Duration) {
        self.deadline = Instant::now() + debounce;
    }
}

/// Timeout for pending `MOVED_FROM` events waiting for matching `MOVED_TO`.
/// Set to 2 seconds to handle scheduling delays under load.
const MOVE_COOKIE_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum entries in the loop prevention LRU cache.
const SKIP_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(10_000) {
    Some(n) => n,
    None => panic!("SKIP_CACHE_SIZE must be > 0"),
};

/// A pending move-from event waiting for a matching move-to.
struct PendingMove {
    old_path: PathBuf,
    source: String,
    old_id: Option<FileId>,
    expires: Instant,
}

// =============================================================================
// Watcher
// =============================================================================

/// Inotify-based file watcher with debouncing.
pub struct Watcher {
    config: WatcherConfig,
    /// Async inotify event stream for reading kernel events.
    stream: EventStream<Vec<u8>>,
    /// Maps watch descriptors to directory path and source name.
    watches: HashMap<WatchDescriptor, WatchInfo>,
    /// Root directories added via `add_recursive`, for rescan on overflow.
    roots: Vec<(PathBuf, String)>,
    /// Files awaiting debounce expiry. Keyed by `FileId` for deduplication.
    pending: HashMap<FileId, PendingEntry>,
    /// Reverse lookup: path -> `FileId`. Enables path-based pending removal.
    pending_by_path: HashMap<PathBuf, FileId>,
    /// `MOVED_FROM` events awaiting matching `MOVED_TO`. Keyed by inotify cookie.
    pending_moves: HashMap<u32, PendingMove>,
    /// Events ready for dispatch via `next()`. Debounce complete or immediate.
    ready: VecDeque<FileEvent>,
    /// LRU cache for loop prevention: `FileId` -> ctime at write.
    /// Skip events only if ctime matches (file unchanged since we wrote it).
    skip_cache: LruCache<FileId, i64>,
    /// Directories to exclude from recursive watching.
    excludes: Vec<PathBuf>,
    /// Overflow backoff: consecutive overflow count.
    overflow_count: u32,
    /// Overflow backoff: time of last overflow (for reset).
    last_overflow: Option<Instant>,
}

impl Watcher {
    /// Create a new watcher with default configuration.
    ///
    /// # Errors
    /// Returns an error if inotify initialization fails.
    pub fn new() -> Result<Self> {
        Self::with_config(WatcherConfig::default())
    }

    /// Create a new watcher with custom configuration.
    ///
    /// # Errors
    /// Returns an error if inotify initialization fails.
    pub fn with_config(config: WatcherConfig) -> Result<Self> {
        let inotify = Inotify::init().context("Failed to initialize inotify")?;
        let buffer = vec![0u8; INOTIFY_BUFFER_SIZE];
        let stream = inotify.into_event_stream(buffer)?;

        Ok(Self {
            config,
            stream,
            watches: HashMap::new(),
            roots: Vec::new(),
            pending: HashMap::new(),
            pending_by_path: HashMap::new(),
            pending_moves: HashMap::new(),
            ready: VecDeque::new(),
            skip_cache: LruCache::new(SKIP_CACHE_SIZE),
            excludes: Vec::new(),
            overflow_count: 0,
            last_overflow: None,
        })
    }

    /// Add a directory tree to watch.
    ///
    /// - `root`: Root directory to watch recursively.
    /// - `source`: Identifier for events from this tree (e.g., producer name).
    ///
    /// # Errors
    /// Returns an error if adding an inotify watch fails for any directory.
    pub fn add_recursive(&mut self, root: &Path, source: &str) -> Result<()> {
        self.roots.push((root.to_path_buf(), source.to_string()));

        let watch_mask = WatchMask::CLOSE_WRITE
            | WatchMask::DELETE
            | WatchMask::MOVED_FROM
            | WatchMask::MOVED_TO
            | WatchMask::CREATE;

        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            if self.is_excluded(&dir) {
                debug!("Excluding directory: {}", dir.display());
                continue;
            }

            let wd = self.stream.watches()
                .add(&dir, watch_mask)
                .with_context(|| format!("Failed to add watch for {}", dir.display()))?;

            self.watches.insert(
                wd,
                WatchInfo {
                    source: source.to_string(),
                    dir_path: dir.clone(),
                },
            );

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        stack.push(entry.path());
                    }
                }
            }
        }

        Ok(())
    }

    /// Set directories to exclude from recursive watching.
    ///
    /// Excluded directories and their subdirectories will not be watched.
    pub fn set_excludes(&mut self, excludes: Vec<PathBuf>) {
        self.excludes = excludes;
    }

    /// Check if a path should be excluded from watching.
    fn is_excluded(&self, path: &Path) -> bool {
        self.excludes.iter().any(|excl| path.starts_with(excl))
    }

    /// Mark an inode as written with its ctime (for loop prevention).
    /// Events are skipped only if ctime matches (file unchanged since we wrote it).
    pub fn mark_written(&mut self, file_id: FileId, ctime: i64) {
        self.skip_cache.put(file_id, ctime);
    }

    /// Get the next file event.
    ///
    /// This method handles debouncing internally. Modified events are only
    /// returned after the debounce period. Delete events are immediate.
    ///
    /// Returns `None` if the inotify stream ends.
    pub async fn next(&mut self) -> Option<FileEvent> {
        loop {
            // Return any ready events first
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }

            // Flush expired pending entries
            self.flush_expired();
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }

            // Flush expired pending moves
            self.flush_expired_moves();

            // Only use timeout when there are pending entries awaiting debounce
            if let Some(timeout) = self.next_timeout() {
                tokio::select! {
                    biased;

                    event_result = self.stream.next() => {
                        self.process_stream_event(event_result)?;
                    }

                    () = tokio::time::sleep(timeout) => {
                        // Timeout - flush expired pending
                    }
                }
            } else {
                // No pending entries - block on inotify stream
                let event_result = self.stream.next().await;
                self.process_stream_event(event_result)?;
            }
        }
    }

    /// Try to queue a file for processing (used during rescan).
    /// Silently skips if file is inaccessible, a symlink, or not a regular file.
    fn try_queue_file(&mut self, path: PathBuf, source: String) {
        let Ok(meta) = fs::symlink_metadata(&path) else { return };
        let ft = meta.file_type();
        if ft.is_symlink() || !ft.is_file() { return; }
        let file_id = (meta.dev(), meta.ino());
        self.handle_modify(file_id, path, source);
    }

    /// Rescan root directories after overflow.
    fn rescan_roots(&mut self, roots: &[(PathBuf, String)]) -> usize {
        let mut count = 0;
        for (root, source) in roots {
            self.rescan_directory(root, source, &mut count);
        }
        count
    }

    fn rescan_directory(&mut self, dir: &Path, source: &str, count: &mut usize) {
        if self.is_excluded(dir) {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let ft = meta.file_type();
            // Only recurse into real directories, not symlinks
            if ft.is_dir() && !ft.is_symlink() {
                self.rescan_directory(&path, source, count);
            } else if ft.is_file() {
                self.try_queue_file(path, source.to_string());
                *count += 1;
            }
        }
    }

    /// Run the watcher event loop, dispatching events to the handler.
    ///
    /// This method runs until the inotify stream ends or an error occurs.
    /// Use `tokio::select!` to add shutdown handling around this function.
    pub async fn run<H: EventHandler>(&mut self, handler: &mut H) {
        while let Some(event) = self.next().await {
            match event.kind {
                FileEventKind::Modified => {
                    let written = handler.on_modified(&event.path, &event.source);
                    for (file_id, ctime) in written {
                        self.mark_written(file_id, ctime);
                    }
                }
                FileEventKind::Deleted => {
                    handler.on_deleted(&event.path, &event.source);
                }
                FileEventKind::Renamed { old_path } => {
                    let written = handler.on_renamed(&event.path, &old_path, &event.source);
                    for (file_id, ctime) in written {
                        self.mark_written(file_id, ctime);
                    }
                }
            }
        }
    }

    // =========================================================================
    // Internal Methods
    // =========================================================================

    /// Process a single event from the inotify stream.
    /// Returns `None` to signal stream end, `Some(())` to continue.
    fn process_stream_event(
        &mut self,
        event_result: Option<Result<inotify::Event<std::ffi::OsString>, std::io::Error>>,
    ) -> Option<()> {
        match event_result {
            Some(Ok(event)) => {
                self.handle_inotify_event(
                    &event.wd,
                    event.mask,
                    event.name,
                    event.cookie,
                );
                Some(())
            }
            Some(Err(e)) => {
                warn!("Inotify read error: {e}");
                Some(())
            }
            None => None,
        }
    }

    fn next_timeout(&self) -> Option<Duration> {
        self.pending.values().map(|p| p.deadline).min().map(|d| {
            d.saturating_duration_since(Instant::now())
                .max(MIN_POLL_INTERVAL)
        })
    }

    fn flush_expired(&mut self) {
        let now = Instant::now();
        let expired: Vec<FileId> = self
            .pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| *id)
            .collect();

        for file_id in expired {
            if let Some(entry) = self.pending.remove(&file_id) {
                self.pending_by_path.remove(&entry.path);
                self.ready.push_back(FileEvent {
                    path: entry.path,
                    source: entry.source,
                    kind: FileEventKind::Modified,
                });
            }
        }
    }

    fn flush_expired_moves(&mut self) {
        let now = Instant::now();
        let expired: Vec<u32> = self
            .pending_moves
            .iter()
            .filter(|(_, pm)| pm.expires <= now)
            .map(|(cookie, _)| *cookie)
            .collect();

        for cookie in expired {
            if let Some(pm) = self.pending_moves.remove(&cookie) {
                // No matching MOVED_TO arrived - treat as delete (moved out of tree)
                debug!("Move cookie {} expired, emitting delete for {}", cookie, pm.old_path.display());
                self.ready.push_back(FileEvent {
                    path: pm.old_path,
                    source: pm.source,
                    kind: FileEventKind::Deleted,
                });
            }
        }
    }

    /// Handle inotify queue overflow with exponential backoff.
    /// Clears state, re-indexes, and scans the file tree.
    fn handle_overflow(&mut self) {
        // Reset overflow count if enough time has passed
        if let Some(last) = self.last_overflow {
            if last.elapsed() >= OVERFLOW_RESET_AFTER {
                self.overflow_count = 0;
            }
        }

        // Calculate exponential backoff: base * 2^count, capped at max
        let backoff = OVERFLOW_BACKOFF_BASE
            .saturating_mul(2_u32.saturating_pow(self.overflow_count));
        let backoff = backoff.min(OVERFLOW_BACKOFF_MAX);

        // We sleep here as we cannot continue processing before rescan
        warn!("Inotify queue overflow #{} - waiting {:?} before rescan", self.overflow_count + 1, backoff);
        std::thread::sleep(backoff);
        self.overflow_count += 1;
        self.last_overflow = Some(Instant::now());

        // Clear state
        self.pending.clear();
        self.pending_by_path.clear();
        self.pending_moves.clear();
        self.ready.clear();

        // Rescan from roots
        let roots = self.roots.clone();
        let count = self.rescan_roots(&roots);
        warn!("Overflow rescan queued {count} files");
    }

    /// Dispatch a single inotify event to the appropriate handler.
    fn handle_inotify_event(
        &mut self,
        wd: &WatchDescriptor,
        mask: EventMask,
        name: Option<std::ffi::OsString>,
        cookie: u32,
    ) {
        // Handle queue overflow
        if mask.contains(EventMask::Q_OVERFLOW) {
            self.handle_overflow();
            return;
        }

        let Some(watch_info) = self.watches.get(wd) else {
            return;
        };
        let Some(name) = name else { return };
        let file_path = watch_info.dir_path.join(name);
        let source = watch_info.source.clone();

        // Process event types
        if mask.contains(EventMask::CREATE) && mask.contains(EventMask::ISDIR) {
            self.add_watch_for_new_dir(&file_path, &source);
            return;
        }
<<<<<<< HEAD
        if mask.contains(EventMask::ISDIR) {
            return;
        }
=======

        if mask.contains(EventMask::ISDIR) { return; }
>>>>>>> 3c87837 (feat(various): tests, docs, fixes)

        if mask.contains(EventMask::DELETE) {
            if let Some(file_id) = self.pending_by_path.remove(&file_path) {
                self.pending.remove(&file_id);
            }
            self.ready.push_back(FileEvent {
                path: file_path,
                source,
                kind: FileEventKind::Deleted,
            });
            return;
        }

        // MOVED_FROM: File is being moved. Store pending move and wait for matching MOVED_TO.
        // If no MOVED_TO arrives (timeout), file was moved out of tree -> emit Delete.
        if mask.contains(EventMask::MOVED_FROM) {
            let old_id = fs::metadata(&file_path).ok().map(|m| (m.dev(), m.ino()));
            self.pending_by_path.remove(&file_path);
            debug!("MOVED_FROM cookie={}: {}", cookie, file_path.display());
            self.pending_moves.insert(
                cookie,
                PendingMove {
                    old_path: file_path,
                    source,
                    old_id,
                    expires: Instant::now() + MOVE_COOKIE_TIMEOUT,
                },
            );
            return;
        }

        // MOVED_TO and CLOSE_WRITE: File content available for processing.
        let is_moved_to = mask.contains(EventMask::MOVED_TO);
        let is_close_write = mask.contains(EventMask::CLOSE_WRITE);
<<<<<<< HEAD
        if !is_moved_to && !is_close_write {
            return;
        }

        let Ok(meta) = fs::metadata(&file_path) else {
            return;
        };
=======

        if !is_moved_to && !is_close_write { return; }
        let Ok(meta) = fs::metadata(&file_path) else { return };

>>>>>>> 3c87837 (feat(various): tests, docs, fixes)
        let file_id = (meta.dev(), meta.ino());
        let ctime = meta.ctime();

        if let Some(&cached_ctime) = self.skip_cache.get(&file_id) {
            if cached_ctime == ctime {
                debug!("Skipping own write: {}", file_path.display());
                if is_moved_to {
                    self.pending_moves.remove(&cookie);
                }
                return;
            }
        }

        // MOVED_TO: Match with pending MOVED_FROM via cookie.
        // - With match + same inode: rename within tree
        // - With match + different inode: file replaced (delete old + scan new)
        // - Without match: file moved in from outside tree (scan as new)
        if is_moved_to {
            if let Some(pm) = self.pending_moves.remove(&cookie) {
                let is_same_inode = pm.old_id == Some(file_id);

                if is_same_inode {
                    // Rename: same inode, different path
                    if let Some(entry) = self.pending.remove(&file_id) {
                        // Was pending scan -> delete old path, rescan at new path
                        self.pending_by_path.remove(&entry.path);
                        debug!("MOVED_TO cookie={cookie}: {} -> {} (pending)", pm.old_path.display(), file_path.display());
                        self.ready.push_back(FileEvent { path: pm.old_path, source: pm.source, kind: FileEventKind::Deleted });
                        // Fall through to handle_modify for new path
                    } else {
<<<<<<< HEAD
                        // File was already scanned - safe to rename without scan
                        debug!(
                            "MOVED_TO cookie={}: {} -> {} (rename, already scanned)",
                            cookie,
                            pm.old_path.display(),
                            file_path.display()
                        );

                        // Emit Renamed event immediately (no debounce needed)
                        self.ready.push_back(FileEvent {
                            path: file_path,
                            source,
                            kind: FileEventKind::Renamed {
                                old_path: pm.old_path,
                            },
                        });
=======
                        // Already scanned -> just emit rename, no rescan needed
                        debug!("MOVED_TO cookie={cookie}: {} -> {} (scanned)", pm.old_path.display(), file_path.display());
                        if let Some(old_id) = self.pending_by_path.remove(&file_path) {
                            self.pending.remove(&old_id);
                        }
                        self.ready.push_back(FileEvent { path: file_path, source, kind: FileEventKind::Renamed { old_path: pm.old_path } });
>>>>>>> 3c87837 (feat(various): tests, docs, fixes)
                        return;
                    }
                } else {
                    // Replace: different inode at same path -> delete old, scan new
                    debug!("MOVED_TO cookie={cookie}: {} -> {} (replace)", pm.old_path.display(), file_path.display());
                    self.ready.push_back(FileEvent { path: pm.old_path, source: pm.source, kind: FileEventKind::Deleted });
                    // Fall through to handle_modify for new file
                }
            } else {
                // No matching MOVED_FROM -> file moved in from outside tree
                debug!("MOVED_TO cookie={}: {} (moved in)", cookie, file_path.display());
                // Fall through to handle_modify
            }
        }

        self.handle_modify(file_id, file_path, source);
    }

    fn add_watch_for_new_dir(&mut self, dir: &Path, source: &str) {
        if self.is_excluded(dir) {
            debug!("Excluding new directory: {}", dir.display());
            return;
        }

        let watch_mask = WatchMask::CLOSE_WRITE
            | WatchMask::DELETE
            | WatchMask::MOVED_FROM
            | WatchMask::MOVED_TO
            | WatchMask::CREATE;

        match self.stream.watches().add(dir, watch_mask) {
            Ok(wd) => {
                self.watches.insert(
                    wd,
                    WatchInfo {
                        source: source.to_string(),
                        dir_path: dir.to_path_buf(),
                    },
                );
                debug!("Added watch for new directory: {}", dir.display());
            }
            Err(e) => {
                warn!("Failed to add watch for new directory {}: {e}", dir.display());
            }
        }
    }

    fn handle_modify(&mut self, file_id: FileId, path: PathBuf, source: String) {
        if let Some(existing) = self.pending.get_mut(&file_id) {
            // Same inode, possibly renamed - update path in reverse map
            if existing.path != path {
                self.pending_by_path.remove(&existing.path);
                self.pending_by_path.insert(path.clone(), file_id);
            }
            existing.reset_deadline(self.config.debounce_duration);
            existing.path.clone_from(&path);
            debug!("Reset debounce for {}", path.display());
        } else {
            // Check if path is already pending with a different inode (file was replaced)
            if let Some(old_id) = self.pending_by_path.remove(&path) {
                self.pending.remove(&old_id);
                debug!("Inode changed for {}, resetting debounce", path.display());
            }

            // Force processing if limit reached
            if self.pending.len() >= self.config.max_pending {
                self.force_process_oldest();
            }
            debug!("Pending: {} (total: {})", path.display(), self.pending.len() + 1);

            self.pending_by_path.insert(path.clone(), file_id);
            self.pending.insert(
                file_id,
                PendingEntry::new(path, source, self.config.debounce_duration),
            );
        }
    }

    fn force_process_oldest(&mut self) {
        let oldest = self
            .pending
            .iter()
            .min_by_key(|(_, p)| p.deadline)
            .map(|(id, _)| *id);

        if let Some(file_id) = oldest {
            if let Some(entry) = self.pending.remove(&file_id) {
                self.pending_by_path.remove(&entry.path);
                warn!("Pending queue full, forcing oldest entry");
                self.ready.push_back(FileEvent {
                    path: entry.path,
                    source: entry.source,
                    kind: FileEventKind::Modified,
                });
            }
        }
    }
}
