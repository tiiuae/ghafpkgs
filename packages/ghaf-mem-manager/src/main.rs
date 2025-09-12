/*
 * Copyright 2025 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::Result;
use clap::Parser;
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, info, warn};

mod qmp;
use qmp::QmpEndpoint;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to QMP socket
    #[arg(short, long)]
    socket: Vec<PathBuf>,

    /// Monitoring interval in seconds
    #[arg(short, long, default_value_t = 1)]
    interval: u64,

    /// Minimum ballooning interval
    #[arg(short, long, default_value_t = 3)]
    balloon_interval: u64,

    /// Minimum memory size
    #[arg(short, long, default_value_t = usize::MIN)]
    minimum: usize,

    /// Maximum memory size
    #[arg(short = 'M', long, default_value_t = usize::MAX)]
    maximum: usize,

    /// Low memory presure
    #[arg(short, long, default_value_t = 70)]
    low: u8,

    /// High memory pressure
    #[arg(short, long, default_value_t = 80)]
    high: u8,
}

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

async fn monitor_memory(args: Args) -> Result<()> {
    let mut qmps: HashMap<_, (_, Option<Instant>)> = args
        .socket
        .iter()
        .map(|p| (QmpEndpoint::new(p), (None, None)))
        .collect();
    let dur = Duration::from_secs(args.interval);
    let bival = Duration::from_secs(args.balloon_interval);
    let mut ival = tokio::time::interval(dur);
    let mut errors = 0;
    ival.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ival.tick().await;
        for (qmp, (last, last_balloon)) in &mut qmps {
            let (conn, task, mut receiver) = match qmp.connect().await {
                Ok(ctr) => ctr,
                Err(e) => {
                    warn!("Connection to {qmp} failed: {e}, trying again later",);
                    continue;
                }
            };
            if let Err(e) = tokio::select! {
                e = async {
                    conn.set_stats_interval(dur).await?;
                    let balloon = conn.query_balloon().await?;
                    let memory = conn.query_memory().await?;
                    let guest_stats = conn.query_stats().await?;

                    if last.replace(guest_stats.last_update) != Some(guest_stats.last_update) {
                        let stats = MemoryStats {
                            balloon_size: balloon.actual,
                            base_memory: memory.base_memory,
                            plugged_memory: memory.plugged_memory,
                            total_memory: memory.base_memory + memory.plugged_memory,
                            free_memory: guest_stats.stats.stat_free_memory,
                            available_memory: guest_stats.stats.stat_available_memory,
                        };

                        debug!("Stats for {qmp}: {stats}, pressure: {}%", stats.pressure());
                        if let Some(target) = stats
                            .window(args.low, args.high)
                            .map(|t| t.clamp(args.minimum, args.maximum))
                            .filter(|&t| t != stats.balloon_size)
                            .filter(|_| last_balloon.is_none_or(|l| l.elapsed() >= bival))
                        {
                            info!("Adjusting {qmp} balloon size from {} to {target}",
                                stats.balloon_size);
                            last_balloon.replace(Instant::now());
                            conn.balloon(target).await?;
                        }
                    }
                    Ok(())
                } => e,
                e = task => e,
                () = {
                    async move {
                        while let Some(e) = receiver.recv().await {
                            info!("Got event: {e:?}");
                        }
                    }
                } => Ok(()),
            } {
                errors += 1;
                if errors >= 5 {
                    Err(e)?;
                } else {
                    warn!("Got error {e} with {qmp} for the {errors}th time");
                }
            } else {
                errors = 0;
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    monitor_memory(args).await
}
