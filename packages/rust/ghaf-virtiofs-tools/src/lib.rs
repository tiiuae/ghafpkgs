// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Shared directories daemon library for cross-VM file sharing.
//!
//! This library provides virus-scanned file sharing between VMs using:
//! - Linux inotify for filesystem monitoring
//! - `ClamAV` for virus scanning
//! - btrfs reflink for zero-copy file propagation
//!
//! # Modules
//!
//! - [`scanner`] - Virus scanning with path-based and stream-based interfaces
//! - [`watcher`] - Inotify watcher with debouncing and event handler trait

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

pub mod scanner;
pub mod util;
pub mod watcher;
