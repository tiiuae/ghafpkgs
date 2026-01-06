// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Guest notification module.
//!
//! Notifies guest VMs over vsock when files are propagated.
//! Guests receive channel name and trigger file browser refresh.
//!
//! Protocol: `channel\n`

use log::{debug, info, warn};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio_vsock::{VsockAddr, VsockStream};

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

/// Notifier for sending file events to guest VMs.
///
/// Maintains a mapping of channel names to guest VMs that should be notified
/// when files are created, renamed, or deleted in that channel.
pub struct Notifier {
    /// Map of channel name -> list of VMs to notify
    targets: HashMap<String, Vec<NotifyTarget>>,
}

impl Notifier {
    /// Create a new notifier from channel -> VM mappings
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // HashMap::new is not const
    pub fn new(targets: HashMap<String, Vec<NotifyTarget>>) -> Self {
        Self { targets }
    }

    /// Create an empty notifier (no-op, for when notifications disabled)
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            targets: HashMap::new(),
        }
    }

    /// Check if notifier has any targets configured
    #[cfg(test)]
    #[must_use]
    pub fn has_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Notify all VMs subscribed to a channel to refresh.
    /// Non-blocking, logs errors but doesn't fail.
    pub async fn notify(&self, channel: &str) {
        let Some(targets) = self.targets.get(channel) else {
            return;
        };

        let message = format!("{channel}\n");

        for target in targets {
            if let Err(e) = Self::send(target, &message).await {
                warn!(
                    "Failed to notify CID {} for channel {}: {}",
                    target.cid, channel, e
                );
            } else {
                debug!("Notified CID {} to refresh {}", target.cid, channel);
            }
        }
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
    let mut targets: HashMap<String, Vec<NotifyTarget>> = HashMap::new();

    for (channel_name, channel_config) in config {
        let Some(ref notify_config) = channel_config.notify else {
            continue;
        };

        if notify_config.guests.is_empty() {
            continue;
        }

        let port = notify_config.port;
        let channel_targets: Vec<NotifyTarget> = notify_config
            .guests
            .iter()
            .map(|&cid| NotifyTarget::new(cid, port))
            .collect();

        if !channel_targets.is_empty() {
            info!(
                "Channel '{}': notifications enabled for {} guests on port {}",
                channel_name,
                channel_targets.len(),
                port
            );
            targets.insert(channel_name.clone(), channel_targets);
        }
    }

    if targets.is_empty() {
        Notifier::disabled()
    } else {
        Notifier::new(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_notifier() {
        let notifier = Notifier::disabled();
        assert!(!notifier.has_targets());
    }

    #[test]
    fn test_notifier_with_targets() {
        let mut targets = HashMap::new();
        targets.insert("test".to_string(), vec![NotifyTarget::new(3, 3401)]);
        let notifier = Notifier::new(targets);
        assert!(notifier.has_targets());
    }
}
