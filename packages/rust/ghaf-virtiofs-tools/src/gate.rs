// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Virtiofs gateway daemon for cross-VM file sharing.

mod config;
mod daemon;
mod notify;
mod sync;

pub use config::{ChannelConfig, verify_config};
pub use daemon::Daemon;
