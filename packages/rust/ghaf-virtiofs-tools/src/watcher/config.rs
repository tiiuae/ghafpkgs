// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Watcher configuration.

use std::path::PathBuf;
use std::time::Duration;

use super::constants::{DEFAULT_DEBOUNCE_DURATION, MOVE_COOKIE_TIMEOUT};

/// Configuration for the watcher.
#[derive(Debug, Clone, Default)]
pub struct WatcherConfig {
    /// Duration to wait after last write before emitting event.
    pub debounce_duration: Duration,
    /// Directories to exclude from recursive watching.
    pub excludes: Vec<PathBuf>,
    /// Timeout for matching `MOVED_FROM` with `MOVED_TO` events.
    /// If no `MOVED_TO` arrives within this duration, emit `Deleted`.
    pub move_cookie_timeout: Duration,
}

impl WatcherConfig {
    /// Create a new config with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            debounce_duration: DEFAULT_DEBOUNCE_DURATION,
            excludes: Vec::new(),
            move_cookie_timeout: MOVE_COOKIE_TIMEOUT,
        }
    }
}
