/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use std::{path::PathBuf, sync::Arc, time::Duration};

pub use anyhow::Error;
use tokio::sync::RwLock;
use tracing::debug;

use crate::qmp;

#[derive(Debug)]
struct MemoryStats {
    balloon_size: usize,
    base_memory: usize,
    plugged_memory: usize,
    total_memory: usize,
    free_memory: usize,
    available_memory: usize,
}

impl MemoryStats {
    pub fn new(
        mem_info: &qmp::MemoryInfo,
        guest_info: &qmp::GuestMemoryInfo,
        bal_info: &qmp::BalloonInfo,
    ) -> Self {
        Self {
            balloon_size: bal_info.actual,
            base_memory: mem_info.base_memory,
            plugged_memory: mem_info.plugged_memory,
            total_memory: mem_info.base_memory + mem_info.plugged_memory,
            free_memory: guest_info.stats.stat_free_memory,
            available_memory: guest_info.stats.stat_available_memory,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn pressure(&self) -> u8 {
        ((201 * self.balloon_size - 200 * self.available_memory) / self.balloon_size / 2) as u8
    }

    pub fn reserved(&self) -> usize {
        self.balloon_size - self.available_memory
    }

    pub fn adjusted(&self, target: u8) -> usize {
        self.reserved() * 100 / target as usize
    }

    pub fn window(&self, min: u8, max: u8) -> Option<usize> {
        let p = self.pressure();
        if p < min {
            Some(self.adjusted(min))
        } else if p > max {
            Some(self.adjusted(max - 2))
        } else {
            None
        }
    }
}

impl std::fmt::Display for MemoryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Memory stats:\n\
             Balloon size: {} MiB\n\
             Base memory: {} MiB\n\
             Plugged memory: {} MiB\n\
             Total memory: {} MiB\n\
             Free memory: {} MiB\n\
             Available memory: {} MiB",
            self.balloon_size / 1024 / 1024,
            self.base_memory / 1024 / 1024,
            self.plugged_memory / 1024 / 1024,
            self.total_memory / 1024 / 1024,
            self.free_memory / 1024 / 1024,
            self.available_memory / 1024 / 1024
        )
    }
}

pub(crate) struct VM {
    endpoint: qmp::Endpoint,
    pub minimum: usize,
    maximum: usize,
    last_update: Arc<RwLock<Option<(usize, usize)>>>,
}

impl std::fmt::Display for VM {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        self.endpoint.fmt(f)
    }
}

impl std::hash::Hash for VM {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.endpoint.hash(state);
    }
}

impl PartialEq for VM {
    fn eq(&self, other: &Self) -> bool {
        self.endpoint == other.endpoint
    }
}

impl Eq for VM {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MemInfo {
    pub current: usize,
    pub preferred: usize,
}

impl VM {
    pub fn new(endpoint: impl Into<PathBuf>, minimum: usize, maximum: usize) -> Self {
        Self {
            endpoint: qmp::Endpoint::new(endpoint),
            minimum,
            maximum,
            last_update: Arc::new(RwLock::new(None)),
        }
    }

    fn clamp(&self, size: usize) -> usize {
        size.clamp(self.minimum, self.maximum)
    }

    async fn calc_preferred(
        &self,
        conn: &qmp::Connection,
        low: u8,
        high: u8,
    ) -> Result<MemInfo, Error> {
        conn.set_stats_interval(Duration::from_secs(5)).await?;
        let balloon = conn.query_balloon().await?;
        let memory = conn.query_memory().await?;
        let guest_stats = conn.query_stats().await?;

        let last = self
            .last_update
            .write()
            .await
            .replace((guest_stats.last_update, balloon.actual));
        match last {
            Some((last, prev)) if last == guest_stats.last_update => Ok(MemInfo {
                current: prev,
                preferred: prev,
            }),
            _ => {
                let stats = MemoryStats::new(&memory, &guest_stats, &balloon);

                debug!(
                    "Stats for {}: {stats}, pressure: {}%",
                    self.endpoint,
                    stats.pressure()
                );
                Ok(MemInfo {
                    current: stats.balloon_size,
                    preferred: stats
                        .window(low, high)
                        .map_or(stats.balloon_size, |t| self.clamp(t)),
                })
            }
        }
    }

    pub async fn adjust(&self, balloon: usize) -> Result<(), Error> {
        let (conn, task, mut receiver) = self.endpoint.connect().await?;
        tokio::select! {
            r = async move {
                conn.balloon(balloon).await
            } => r,
            e = task => e.and_then(|()| Err(anyhow::anyhow!("Task stopped unexpectedly"))),
            () = async move {
                while receiver.recv().await.is_some() {}
            } => Err(anyhow::anyhow!("Event channel closed unexpectedly")),
        }
    }

    pub async fn preferred_memory_size(&self, low: u8, high: u8) -> Result<MemInfo, Error> {
        let (conn, task, mut receiver) = self.endpoint.connect().await?;
        tokio::select! {
            r = self.calc_preferred(&conn, low, high) => r,
            e = task => e.and_then(|()| Err(anyhow::anyhow!("Task stopped unexpectedly"))),
            () = async move {
                while receiver.recv().await.is_some() {}
            } => Err(anyhow::anyhow!("Event channel closed unexpectedly")),
        }
    }
}
