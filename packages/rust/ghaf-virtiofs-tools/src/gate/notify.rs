// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use log::{debug, info, warn};
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
    pub const fn new(targets: HashMap<String, Vec<NotifyTarget>>) -> Self {
        Self { targets }
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
