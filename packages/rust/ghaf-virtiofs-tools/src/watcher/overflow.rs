// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Overflow detection and recovery.
//!
//! See `README.md` for the full recovery strategy documentation.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use log::warn;

use super::constants::{
    MAX_TOTAL_QUEUED, OVERFLOW_BACKOFF_BASE, OVERFLOW_BACKOFF_MAX, OVERFLOW_RESET_AFTER,
};
use super::core::Watcher;

/// Reason for triggering overflow recovery.
#[derive(Debug, Clone, Copy)]
pub(super) enum OverflowReason {
    /// Kernel inotify queue overflow (`IN_Q_OVERFLOW`).
    KernelQueue,
    /// Application memory pressure exceeded.
    MemoryPressure,
}

impl Watcher {
    pub(super) fn memory_pressure_exceeded(&self) -> bool {
        let total = self.ready.len() + self.pending.len() + self.pending_moves.len();
        total >= MAX_TOTAL_QUEUED
    }

    pub(super) fn handle_overflow(&mut self, reason: OverflowReason) {
        if let Some(last) = self.last_overflow {
            if last.elapsed() >= OVERFLOW_RESET_AFTER {
                self.overflow_count = 0;
            }
        }

        let backoff =
            OVERFLOW_BACKOFF_BASE.saturating_mul(2_u32.saturating_pow(self.overflow_count));
        let backoff = backoff.min(OVERFLOW_BACKOFF_MAX);

        let now = Instant::now();
        warn!(
            "{:?} overflow #{} - pausing {:?} before recovery",
            reason,
            self.overflow_count + 1,
            backoff
        );
        self.overflow_count += 1;
        self.last_overflow = Some(now);

        self.pending.clear();
        self.pending.shrink_to_fit();
        self.pending_by_path.clear();
        self.pending_by_path.shrink_to_fit();
        self.pending_moves.clear();
        self.pending_moves.shrink_to_fit();
        self.ready.clear();
        self.ready.shrink_to_fit();
        self.skip_cache.clear();
        self.watches.clear();
        self.watches.shrink_to_fit();

        self.recovery_until = Some(now + backoff);
    }

    /// Re-establish watches for all root directories after overflow.
    ///
    /// Only re-adds inotify watches - does NOT enumerate or queue files.
    /// Returns the number of directories watched.
    pub(super) fn rewatch_roots(&mut self) -> usize {
        let roots = std::mem::take(&mut self.roots);
        let mut count = 0;
        for (root, source) in &roots {
            self.rewatch_directory(root, source, &mut count);
        }
        self.roots = roots;
        count
    }

    fn rewatch_directory(&mut self, dir: &Path, source: &Arc<str>, count: &mut usize) {
        if self.is_excluded(dir) {
            return;
        }

        self.add_watch_for_new_dir(dir, source);
        *count += 1;

        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() && !ft.is_symlink() {
                self.rewatch_directory(&entry.path(), source, count);
            }
        }
    }
}
