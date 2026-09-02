// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Inotify-based file watcher with debouncing.
//!
//! This module provides a recursive file watcher that:
//! - Watches directory trees for file changes
//! - Debounces rapid modifications to avoid duplicate events
//! - Handles file moves within and across watched trees
//! - Recovers gracefully from overflow conditions

mod config;
mod constants;
mod core;
mod event;
mod overflow;
mod pending;
#[cfg(test)]
mod tests;

pub use config::WatcherConfig;
pub use constants::DEFAULT_DEBOUNCE_DURATION;
pub use core::Watcher;
pub use event::{EventHandler, FileEvent, FileEventKind, FileId};
