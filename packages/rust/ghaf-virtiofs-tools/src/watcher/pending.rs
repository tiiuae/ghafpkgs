// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Debounce state and pending entry management.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::debug;

use super::constants::{DEFAULT_MAX_PENDING, MIN_POLL_INTERVAL, SHRINK_MIN_CAPACITY};
use super::core::Watcher;
use super::event::{FileEvent, FileEventKind, FileId};

/// State of a path in the pending tracking system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathState {
    /// File is tracked with known identity (device, inode).
    Tracked(FileId),
    /// `CLOSE_WRITE` received but metadata unavailable.
    /// File may have been renamed before we could stat it.
    MetadataUnavailable,
}

/// A file pending event emission after debounce.
pub(super) struct PendingEntry {
    pub(super) path: PathBuf,
    pub(super) source: Arc<str>,
    pub(super) deadline: Instant,
}

impl PendingEntry {
    pub(super) fn new(path: PathBuf, source: Arc<str>, debounce: Duration) -> Self {
        Self {
            path,
            source,
            deadline: Instant::now() + debounce,
        }
    }

    pub(super) fn reset_deadline(&mut self, debounce: Duration) {
        self.deadline = Instant::now() + debounce;
    }
}

/// A pending move-from event waiting for a matching move-to.
pub(super) struct PendingMove {
    pub(super) old_path: PathBuf,
    pub(super) source: Arc<str>,
    /// State of the file when `MOVED_FROM` arrived.
    /// Some(Tracked(id)) = file was debouncing with known identity
    /// Some(MetadataUnavailable) = file had pending write but no metadata
    /// None = file was idle (not pending)
    pub(super) pending_state: Option<PathState>,
    pub(super) expires: Instant,
}

impl Watcher {
    pub(super) fn next_timeout(&self) -> Option<Duration> {
        // Find earliest deadline from both pending debounces and pending moves
        let pending_deadline = self.pending.values().map(|p| p.deadline).min();
        let moves_deadline = self.pending_moves.values().map(|m| m.expires).min();

        let earliest = match (pending_deadline, moves_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, None) => a,
            (None, b) => b,
        };

        earliest.map(|d| {
            Some(d.saturating_duration_since(Instant::now()))
                .filter(|r| !r.is_zero())
                .unwrap_or(MIN_POLL_INTERVAL)
        })
    }

    pub(super) fn flush_expired(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .pending
            .extract_if(|_, p| p.deadline <= now)
            .map(|(_, e)| e)
            .collect();

        for PendingEntry { path, source, .. } in expired {
            self.pending_by_path.remove(&path);
            self.ready.push_back(FileEvent {
                path,
                source,
                kind: FileEventKind::Modified,
            });
        }

        self.maybe_shrink_collections();
    }

    pub(super) fn flush_expired_moves(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .pending_moves
            .extract_if(|_, pm| pm.expires <= now)
            .collect();

        for (
            cookie,
            PendingMove {
                old_path,
                source,
                pending_state,
                ..
            },
        ) in expired
        {
            debug!(
                "Move cookie {} expired, emitting delete for {}",
                cookie,
                old_path.display()
            );

            // If file was debouncing with known identity, clean up pending entry
            if let Some(PathState::Tracked(file_id)) = pending_state {
                self.pending.remove(&file_id);
            }

            self.ready.push_back(FileEvent {
                path: old_path,
                source,
                kind: FileEventKind::Deleted,
            });
        }
    }

    pub(super) fn handle_modify(&mut self, file_id: FileId, path: PathBuf, source: Arc<str>) {
        if let Some(existing) = self.pending.get_mut(&file_id) {
            if existing.path != path {
                self.pending_by_path.remove(&existing.path);
                self.pending_by_path
                    .insert(path.clone(), PathState::Tracked(file_id));
            }
            existing.reset_deadline(self.config.debounce_duration);
            existing.path.clone_from(&path);
            debug!("Reset debounce for {}", path.display());
        } else {
            if let Some(PathState::Tracked(old_id)) = self.pending_by_path.remove(&path) {
                self.pending.remove(&old_id);
                debug!("Inode changed for {}, resetting debounce", path.display());
            }

            if self.pending.len() >= DEFAULT_MAX_PENDING {
                self.force_process_oldest();
            }
            debug!(
                "Pending: {} (total: {})",
                path.display(),
                self.pending.len() + 1
            );

            self.pending_by_path
                .insert(path.clone(), PathState::Tracked(file_id));
            self.pending.insert(
                file_id,
                PendingEntry::new(path, source, self.config.debounce_duration),
            );
        }
    }

    pub(super) fn force_process_oldest(&mut self) {
        let oldest = self
            .pending
            .iter()
            .min_by_key(|(_, p)| p.deadline)
            .map(|(id, _)| *id);

        if let Some(file_id) = oldest {
            if let Some(entry) = self.pending.remove(&file_id) {
                self.pending_by_path.remove(&entry.path);
                debug!("Pending queue full, forcing oldest entry");
                self.ready.push_back(FileEvent {
                    path: entry.path,
                    source: entry.source,
                    kind: FileEventKind::Modified,
                });
            }
        }
    }

    pub(super) fn maybe_shrink_collections(&mut self) {
        // Shrink when utilization < 25%, target next power of two with 2x headroom
        let pending_len = self.pending.len();
        let pending_cap = self.pending.capacity();
        if pending_len < pending_cap / 4 && pending_cap > SHRINK_MIN_CAPACITY {
            let target = (pending_len * 2)
                .next_power_of_two()
                .max(SHRINK_MIN_CAPACITY);
            self.pending.shrink_to(target);
            self.pending_by_path.shrink_to(target);
            let new_cap = self.pending.capacity();
            if new_cap < pending_cap {
                debug!("Shrunk pending collections: {pending_cap} -> {new_cap}");
            }
        }

        let moves_len = self.pending_moves.len();
        let moves_cap = self.pending_moves.capacity();
        if moves_len < moves_cap / 4 && moves_cap > SHRINK_MIN_CAPACITY {
            let target = (moves_len * 2).next_power_of_two().max(SHRINK_MIN_CAPACITY);
            self.pending_moves.shrink_to(target);
        }

        let ready_len = self.ready.len();
        let ready_cap = self.ready.capacity();
        if ready_len < ready_cap / 4 && ready_cap > SHRINK_MIN_CAPACITY {
            let target = (ready_len * 2).next_power_of_two().max(SHRINK_MIN_CAPACITY);
            self.ready.shrink_to(target);
        }

        let watches_len = self.watches.len();
        let watches_cap = self.watches.capacity();
        if watches_len < watches_cap / 4 && watches_cap > SHRINK_MIN_CAPACITY {
            let target = (watches_len * 2)
                .next_power_of_two()
                .max(SHRINK_MIN_CAPACITY);
            self.watches.shrink_to(target);
        }
    }
}
