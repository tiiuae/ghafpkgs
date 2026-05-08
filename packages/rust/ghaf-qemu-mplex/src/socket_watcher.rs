/*
 * SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use log::info;
use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

pub struct SocketWatcher {
    _watcher: RecommendedWatcher,
    events: mpsc::UnboundedReceiver<()>,
}

impl SocketWatcher {
    pub fn new(qemu_socket: &Path) -> Result<Self> {
        let watched_dir = qemu_socket.parent().ok_or_else(|| {
            anyhow!(
                "qemu socket path has no parent directory: {}",
                qemu_socket.display()
            )
        })?;
        ensure!(
            watched_dir.is_dir(),
            "qemu socket parent directory must exist at startup: {}",
            watched_dir.display()
        );
        let expected_entry = qemu_socket.file_name().ok_or_else(|| {
            anyhow!(
                "qemu socket path has no filename component: {}",
                qemu_socket.display()
            )
        })?;
        let expected_entry = expected_entry.to_os_string();
        let watched_entry = expected_entry.clone();

        let (events_tx, events) = mpsc::unbounded_channel();
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| {
                let Ok(event) = event else {
                    let _ = events_tx.send(());
                    return;
                };

                let matches_expected = if event.paths.is_empty() {
                    true
                } else {
                    event.paths.iter().any(|path| {
                        path.file_name()
                            .is_some_and(|name| name == watched_entry.as_os_str())
                    })
                };

                if matches_expected {
                    let _ = events_tx.send(());
                }
            },
            NotifyConfig::default(),
        )
        .context("failed to initialize qemu socket watcher")?;

        watcher
            .watch(watched_dir, RecursiveMode::NonRecursive)
            .with_context(|| {
                format!(
                    "failed to watch qemu socket parent directory: {}",
                    watched_dir.display()
                )
            })?;

        info!(
            "qemu_socket={} watching_qemu_socket parent={} entry={}",
            qemu_socket.display(),
            watched_dir.display(),
            expected_entry.to_string_lossy(),
        );

        Ok(Self {
            _watcher: watcher,
            events,
        })
    }

    pub async fn wait_for_change(&mut self) -> Result<()> {
        self.events
            .recv()
            .await
            .context("qemu socket watcher channel closed unexpectedly")?;

        while self.events.try_recv().is_ok() {}

        Ok(())
    }
}
