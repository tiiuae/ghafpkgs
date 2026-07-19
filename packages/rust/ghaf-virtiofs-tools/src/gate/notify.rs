// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Guest VM notification system with backoff and debouncing.
//!
//! Sends refresh notifications to guest VMs via VSOCK when files change.
//! Includes protection against log spam and resource waste when VMs are
//! unreachable.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio_vsock::{VsockAddr, VsockStream};

/// How long to skip a CID after a connection failure.
const CID_BACKOFF_DURATION: Duration = Duration::from_secs(10);

/// Minimum interval between notifications to the same (channel, CID).
const NOTIFY_DEBOUNCE: Duration = Duration::from_secs(1);

/// Guest VM target for notifications
#[derive(Debug, Clone)]
pub struct NotifyTarget {
    pub cid: u32,
    pub port: u32,
}

impl NotifyTarget {
    pub const fn new(cid: u32, port: u32) -> Self {
        Self { cid, port }
    }
}

/// Tracks failure and notification state for a CID.
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
struct CidState {
    /// When a connection to this CID last failed.
    failure_at: Option<Instant>,
    /// When we last logged a warning for this CID.
    warning_at: Option<Instant>,
    /// When we last notified each channel.
    notify_at: HashMap<String, Instant>,
}

/// Notifier for sending file events to guest VMs.
///
/// Maintains a mapping of channel names to guest VMs that should be notified
/// when files are created, renamed, or deleted in that channel.
///
/// Includes:
/// - **Backoff**: Skip CIDs that failed recently (10s)
/// - **Rate-limited warnings**: Log at most once per backoff period per CID
/// - **Debouncing**: Skip duplicate notifications within 1s
pub struct Notifier {
    /// Map of channel name -> list of VMs to notify
    targets: HashMap<String, Vec<NotifyTarget>>,
    /// Per-CID state tracking (failures, last notify times)
    state: RwLock<HashMap<u32, CidState>>,
}

impl Notifier {
    /// Create a new notifier from channel -> VM mappings
    pub fn new(targets: HashMap<String, Vec<NotifyTarget>>) -> Self {
        Self {
            targets,
            state: RwLock::new(HashMap::new()),
        }
    }

    /// Notify all VMs subscribed to a channel to refresh.
    /// Non-blocking, logs errors but doesn't fail.
    ///
    /// Skips CIDs that:
    /// - Failed to connect within the last 30 seconds
    /// - Were notified for this channel within the last 1 second
    pub async fn notify(&self, channel: &str) {
        let Some(targets) = self.targets.get(channel) else {
            return;
        };

        let message = format!("{channel}\n");
        let now = Instant::now();

        for target in targets {
            // Check if we should skip this CID
            if self.should_skip(target.cid, channel, now).await {
                continue;
            }

            // Attempt to send notification
            if let Err(e) = Self::send(target, &message).await {
                self.record_failure(target.cid, channel, now, &e).await;
            } else {
                self.record_success(target.cid, channel, now).await;
                debug!("Notified CID {} to refresh {}", target.cid, channel);
            }
        }
    }

    /// Check if we should skip notifying this CID.
    #[allow(clippy::significant_drop_tightening)]
    async fn should_skip(&self, cid: u32, channel: &str, now: Instant) -> bool {
        // Extract check results while holding lock, then release immediately
        let (in_backoff, in_debounce) = {
            let state = self.state.read().await;
            let Some(cid_state) = state.get(&cid) else {
                return false;
            };

            let in_backoff = cid_state
                .failure_at
                .is_some_and(|t| now.duration_since(t) < CID_BACKOFF_DURATION);

            let in_debounce = cid_state
                .notify_at
                .get(channel)
                .is_some_and(|t| now.duration_since(*t) < NOTIFY_DEBOUNCE);

            (in_backoff, in_debounce)
        };

        if in_backoff {
            return true;
        }

        if in_debounce {
            return true;
        }

        false
    }

    /// Record a successful notification.
    #[allow(clippy::significant_drop_tightening)]
    async fn record_success(&self, cid: u32, channel: &str, now: Instant) {
        let mut state = self.state.write().await;
        let cid_state = state.entry(cid).or_default();

        // Clear failure state on success
        cid_state.failure_at = None;
        cid_state.warning_at = None;

        // Update last notify time for this channel
        cid_state.notify_at.insert(channel.to_string(), now);
    }

    /// Record a failed notification attempt.
    #[allow(clippy::significant_drop_tightening)]
    async fn record_failure(&self, cid: u32, channel: &str, now: Instant, error: &std::io::Error) {
        let mut state = self.state.write().await;
        let cid_state = state.entry(cid).or_default();

        // Check if we should log a warning (rate-limited)
        let should_warn = cid_state
            .warning_at
            .is_none_or(|t| now.duration_since(t) >= CID_BACKOFF_DURATION);

        if should_warn {
            warn!(
                "Failed to notify CID {} for channel {}: {} (suppressing for {}s)",
                cid,
                channel,
                error,
                CID_BACKOFF_DURATION.as_secs()
            );
            cid_state.warning_at = Some(now);
        }

        cid_state.failure_at = Some(now);
    }

    /// Send notification to a specific VM
    async fn send(target: &NotifyTarget, message: &str) -> std::io::Result<()> {
        let addr = VsockAddr::new(target.cid, target.port);
        let mut stream = VsockStream::connect(addr).await?;
        stream.write_all(message.as_bytes()).await?;
        stream.shutdown(std::net::Shutdown::Write)?;
        Ok(())
    }
}

/// Build notifier from channel configurations
pub fn build_notifier(config: &super::config::Config) -> Notifier {
    Notifier::new(
        config
            .iter()
            .filter_map(|(name, config)| {
                Some((
                    name,
                    config
                        .guest_notify
                        .as_ref()
                        .filter(|gn| !gn.guests.is_empty())?,
                ))
            })
            .inspect(|(name, gn)| {
                info!(
                    "Channel '{}': guest notifications enabled for {} VMs on port {}",
                    name,
                    gn.guests.len(),
                    gn.port
                );
            })
            .map(|(name, gn_config)| {
                (
                    name.clone(),
                    gn_config
                        .guests
                        .iter()
                        .map(|&cid| NotifyTarget::new(cid, gn_config.port))
                        .collect(),
                )
            })
            .collect(),
    )
}
