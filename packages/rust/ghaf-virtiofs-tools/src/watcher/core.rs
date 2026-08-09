// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Core watcher implementation. See `README.md` for architecture.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures::StreamExt;
use inotify::{EventMask, EventStream, Inotify, WatchDescriptor, WatchMask};
use log::{debug, warn};
use lru::LruCache;

use super::config::WatcherConfig;
use super::constants::{INOTIFY_BUFFER_SIZE, SKIP_CACHE_SIZE};
use super::event::{EventHandler, FileEvent, FileEventKind, FileId};
use super::overflow::OverflowReason;
use super::pending::{PathState, PendingEntry, PendingMove};

/// Events we subscribe to for each watched directory.
const WATCH_EVENTS: WatchMask = WatchMask::CLOSE_WRITE
    .union(WatchMask::DELETE)
    .union(WatchMask::MOVED_FROM)
    .union(WatchMask::MOVED_TO)
    .union(WatchMask::CREATE);

/// Metadata for a watched directory.
pub(super) struct WatchInfo {
    /// Source identifier (e.g., producer name).
    pub(super) source: Arc<str>,
    /// Path of the watched directory.
    pub(super) dir_path: PathBuf,
}

/// Inotify-based file watcher with debouncing.
pub struct Watcher {
    /// User-provided configuration.
    pub(super) config: WatcherConfig,

    /// Async inotify event stream.
    pub(super) stream: EventStream<Vec<u8>>,

    /// Active watch descriptors mapped to directory metadata.
    pub(super) watches: HashMap<WatchDescriptor, WatchInfo>,

    /// Root directories being watched: (path, source).
    pub(super) roots: Vec<(PathBuf, Arc<str>)>,

    // Debounce state
    /// Files waiting for debounce timeout. Keyed by (device, inode).
    pub(super) pending: HashMap<FileId, PendingEntry>,
    /// Reverse lookup: path -> state. For cancellation on delete/move.
    pub(super) pending_by_path: HashMap<PathBuf, PathState>,

    // Move tracking
    /// `MOVED_FROM` events awaiting matching `MOVED_TO`. Keyed by cookie.
    pub(super) pending_moves: HashMap<u32, PendingMove>,

    // Output
    /// Events ready to be returned by `next()`.
    pub(super) ready: VecDeque<FileEvent>,

    // Loop prevention
    /// LRU cache of (`FileId`, ctime) for files we wrote. Skip if ctime matches.
    pub(super) skip_cache: LruCache<FileId, i64>,

    // Overflow recovery
    /// Consecutive overflow count (for exponential backoff).
    pub(super) overflow_count: u32,
    /// Last overflow timestamp (for resetting backoff).
    pub(super) last_overflow: Option<Instant>,
    /// If set, sleep until this instant then rescan all roots.
    pub(super) recovery_until: Option<Instant>,
}

// =============================================================================
// Public API
// =============================================================================

impl Watcher {
    /// Create a new watcher with default configuration.
    ///
    /// # Errors
    /// Returns an error if inotify initialization fails.
    pub fn new() -> Result<Self> {
        Self::with_config(WatcherConfig::new())
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
            overflow_count: 0,
            last_overflow: None,
            recovery_until: None,
        })
    }

    /// Add a directory tree to watch.
    ///
    /// - `root`: Root directory to watch recursively.
    /// - `source`: Identifier for events from this tree (e.g., producer name).
    ///
    /// # Errors
    /// Returns an error if:
    /// - The root is already being watched (duplicate root)
    /// - Adding an inotify watch fails for any directory
    pub fn add_recursive(&mut self, root: &Path, source: &str) -> Result<()> {
        if self.roots.iter().any(|(p, _)| p == root) {
            anyhow::bail!("Root already watched: {}", root.display());
        }
        let source: Arc<str> = source.into();
        self.roots.push((root.to_path_buf(), Arc::clone(&source)));

        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            if self.is_excluded(&dir) {
                debug!("Excluding directory: {}", dir.display());
                continue;
            }

            let wd = self
                .stream
                .watches()
                .add(&dir, WATCH_EVENTS)
                .with_context(|| format!("Failed to add watch for {}", dir.display()))?;

            if self.watches.contains_key(&wd) {
                anyhow::bail!(
                    "Directory already watched: {} (overlapping roots?)",
                    dir.display()
                );
            }

            self.watches.insert(
                wd,
                WatchInfo {
                    source: Arc::clone(&source),
                    dir_path: dir.clone(),
                },
            );

            if let Ok(entries) = fs::read_dir(&dir) {
                stack.extend(
                    entries
                        .flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                        .map(|e| e.path()),
                );
            }
        }

        Ok(())
    }

    /// Get the next file event.
    ///
    /// Handles debouncing internally: Modified events are only returned
    /// after the debounce period. Delete events are immediate.
    ///
    /// Returns `None` if the inotify stream ends.
    pub async fn next(&mut self) -> Option<FileEvent> {
        loop {
            // Recovery: sleep then re-establish watches after overflow
            if let Some(until) = self.recovery_until {
                let now = Instant::now();
                if now < until {
                    tokio::time::sleep(until - now).await;
                }
                self.recovery_until = None;
                let count = self.rewatch_roots();
                warn!("Recovery complete, re-established {count} watches");
            }

            // Check memory pressure before processing
            if self.memory_pressure_exceeded() {
                self.handle_overflow(OverflowReason::MemoryPressure);
                continue;
            }

            // Move expired debounced entries to ready queue
            self.flush_expired();
            self.flush_expired_moves();

            // Return next ready event
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }

            // Wait for kernel event or debounce timeout
            let timeout = self.next_timeout().unwrap_or(Duration::MAX);
            if let Ok(e) = tokio::time::timeout(timeout, self.stream.next()).await {
                self.process_stream_event(e?);
            }
        }
    }

    /// Run the watcher event loop, dispatching events to the handler.
    ///
    /// Runs until the inotify stream ends. Use `tokio::select!` for shutdown.
    pub async fn run<H: EventHandler>(&mut self, handler: &mut H) {
        while let Some(event) = self.next().await {
            match event.kind {
                FileEventKind::Modified => {
                    let written = handler.on_modified(&event.path, &event.source);
                    for (file_id, ctime) in written {
                        self.skip_cache.put(file_id, ctime);
                    }
                }
                FileEventKind::Deleted => {
                    handler.on_deleted(&event.path, &event.source);
                }
                FileEventKind::Renamed { old_path } => {
                    let written = handler.on_renamed(&event.path, &old_path, &event.source);
                    for (file_id, ctime) in written {
                        self.skip_cache.put(file_id, ctime);
                    }
                }
            }
        }
    }
}

// =============================================================================
// Event Processing
// =============================================================================

impl Watcher {
    /// Process a raw inotify stream event.
    pub(super) fn process_stream_event(
        &mut self,
        event_result: Result<inotify::Event<std::ffi::OsString>, std::io::Error>,
    ) {
        match event_result {
            Ok(event) => {
                self.handle_inotify_event(&event.wd, event.mask, event.name, event.cookie);
            }
            Err(e) => {
                warn!("Inotify read error: {e}");
            }
        }
    }

    /// Dispatch an inotify event by type.
    fn handle_inotify_event(
        &mut self,
        wd: &WatchDescriptor,
        mask: EventMask,
        name: Option<std::ffi::OsString>,
        cookie: u32,
    ) {
        // System events (no file name)
        if mask.contains(EventMask::Q_OVERFLOW) {
            self.handle_overflow(OverflowReason::KernelQueue);
            return;
        }
        if mask.contains(EventMask::IGNORED) {
            if self.watches.remove(wd).is_some() {
                debug!("Removed stale watch (IGNORED event)");
            }
            return;
        }

        // Resolve watch descriptor to path
        let Some(watch_info) = self.watches.get(wd) else {
            return;
        };
        let Some(name) = name else {
            return;
        };
        let path = watch_info.dir_path.join(name);
        let source = Arc::clone(&watch_info.source);

        // Directory events
        if mask.contains(EventMask::ISDIR) {
            if mask.contains(EventMask::CREATE) {
                self.add_watch_for_new_dir(&path, &source);
            }
            // Ignore other directory events (except DELETE handled below)
            if !mask.contains(EventMask::DELETE) {
                return;
            }
        }

        // File events
        if mask.contains(EventMask::DELETE) {
            self.handle_delete(path, source);
        } else if mask.contains(EventMask::MOVED_FROM) {
            self.handle_moved_from(cookie, path, source);
        } else if mask.contains(EventMask::MOVED_TO) || mask.contains(EventMask::CLOSE_WRITE) {
            self.handle_file_available(mask, cookie, path, source);
        }
    }

    /// Handle DELETE event: emit immediately, cancel any pending debounce.
    fn handle_delete(&mut self, path: PathBuf, source: Arc<str>) {
        if let Some(PathState::Tracked(file_id)) = self.pending_by_path.remove(&path) {
            self.pending.remove(&file_id);
        }
        self.ready.push_back(FileEvent {
            path,
            source,
            kind: FileEventKind::Deleted,
        });
    }

    /// Handle `MOVED_FROM`: store in `pending_moves`, await matching `MOVED_TO`.
    ///
    /// Tracks whether the file was PENDING (awaiting debounce) when moved.
    /// The `MOVED_TO` handler uses this to determine output:
    /// - PENDING file -> Deleted(old) + Modified(new) (forces scan)
    /// - IDLE file -> Renamed(old, new)
    fn handle_moved_from(&mut self, cookie: u32, path: PathBuf, source: Arc<str>) {
        // Check if file was pending - store state for cleanup when cookie expires or MOVED_TO arrives
        let pending_state = self.pending_by_path.remove(&path);
        debug!(
            "MOVED_FROM cookie={}: {} (pending_state={:?})",
            cookie,
            path.display(),
            pending_state
        );
        self.pending_moves.insert(
            cookie,
            PendingMove {
                old_path: path,
                source,
                pending_state,
                expires: Instant::now() + self.config.move_cookie_timeout,
            },
        );
    }

    /// Handle `MOVED_TO` or `CLOSE_WRITE`: file content is available.
    fn handle_file_available(
        &mut self,
        mask: EventMask,
        cookie: u32,
        path: PathBuf,
        source: Arc<str>,
    ) {
        // Get file metadata for identity check
        let meta = fs::metadata(&path);

        // CLOSE_WRITE without metadata: file was renamed before we processed the event.
        // Add to pending_by_path so MOVED_FROM can detect it was pending.
        // Don't add to pending HashMap - MOVED_TO will call handle_modify with valid metadata.
        if mask.contains(EventMask::CLOSE_WRITE) && meta.is_err() {
            debug!(
                "CLOSE_WRITE metadata unavailable (file renamed?): {}",
                path.display()
            );
            self.pending_by_path
                .insert(path, PathState::MetadataUnavailable);
            return;
        }

        // MOVED_TO: try to match with pending MOVED_FROM
        // Handle this even if metadata fails (file may have been deleted after rename)
        if mask.contains(EventMask::MOVED_TO) {
            if let Some(pm) = self.pending_moves.remove(&cookie) {
                // Clear any pending entry at new path (different file being replaced)
                if let Some(PathState::Tracked(old_id)) = self.pending_by_path.remove(&path) {
                    self.pending.remove(&old_id);
                }

                // Clean up pending entry using stored FileId
                if let Some(PathState::Tracked(pending_id)) = pm.pending_state {
                    self.pending.remove(&pending_id);
                }

                if pm.pending_state.is_some() {
                    // File was PENDING - emit Deleted(old)
                    debug!(
                        "MOVED_TO cookie={cookie}: {} -> {} (was pending)",
                        pm.old_path.display(),
                        path.display()
                    );
                    self.ready.push_back(FileEvent {
                        path: pm.old_path,
                        source: pm.source,
                        kind: FileEventKind::Deleted,
                    });
                    // If file still exists, queue for debounce; otherwise we're done
                    if let Ok(ref m) = meta {
                        let file_id = (m.dev(), m.ino());
                        self.handle_modify(file_id, path, source);
                    }
                } else {
                    // File was IDLE - emit Renamed (no metadata needed)
                    debug!(
                        "MOVED_TO cookie={cookie}: {} -> {} (idle, rename only)",
                        pm.old_path.display(),
                        path.display()
                    );
                    self.ready.push_back(FileEvent {
                        path,
                        source,
                        kind: FileEventKind::Renamed {
                            old_path: pm.old_path,
                        },
                    });
                }
                return;
            }
            // No cookie match - treat as move-in (needs metadata for debounce)
            debug!("MOVED_TO cookie={}: {} (moved in)", cookie, path.display());
        }

        let Ok(meta) = meta else {
            return;
        };
        let file_id = (meta.dev(), meta.ino());
        let ctime = meta.ctime();

        // Skip if this is our own write (loop prevention)
        if let Some(&cached_ctime) = self.skip_cache.get(&file_id) {
            if cached_ctime == ctime {
                debug!("Skipping own write: {}", path.display());
                return;
            }
        }

        // Queue for debounce
        self.handle_modify(file_id, path, source);
    }
}

// =============================================================================
// Watch Management
// =============================================================================

impl Watcher {
    /// Add a watch for a newly created directory.
    pub(super) fn add_watch_for_new_dir(&mut self, dir: &Path, source: &Arc<str>) {
        if self.is_excluded(dir) {
            debug!("Excluding new directory: {}", dir.display());
            return;
        }

        match self.stream.watches().add(dir, WATCH_EVENTS) {
            Ok(wd) => {
                self.watches.insert(
                    wd,
                    WatchInfo {
                        source: Arc::clone(source),
                        dir_path: dir.to_path_buf(),
                    },
                );
                debug!("Added watch for new directory: {}", dir.display());
            }
            Err(e) => {
                warn!(
                    "Failed to add watch for new directory {}: {e}",
                    dir.display()
                );
            }
        }
    }

    /// Check if a path matches any exclude pattern.
    pub(super) fn is_excluded(&self, path: &Path) -> bool {
        self.config
            .excludes
            .iter()
            .any(|excl| path.starts_with(excl))
    }
}
