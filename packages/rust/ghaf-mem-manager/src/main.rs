/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, info, warn};

mod crosvm;
mod qmp;
use crosvm::CrosvmEndpoint;
use qmp::QmpEndpoint;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum Hypervisor {
    Crosvm,
    #[default]
    Qemu,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the VM control socket
    #[arg(short, long)]
    socket: Vec<PathBuf>,

    /// Hypervisor control protocol
    #[arg(long, value_enum, default_value_t)]
    hypervisor: Hypervisor,

    /// Path to the crosvm executable
    #[arg(long, default_value = "crosvm")]
    crosvm_binary: PathBuf,

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
    /// Both `balloon_size` (QMP `query-balloon`) and `available_memory` (the
    /// virtio-balloon stats queue) are written by the guest, so neither can be
    /// trusted to be in range. A guest reporting `available_memory` above the
    /// balloon size used to underflow the subtraction, and release builds have
    /// overflow checks off, so the wrap silently produced an attacker-chosen
    /// pressure value that steered the ballooning decision; a balloon size of
    /// zero panicked on the division. Clamp both instead.
    #[allow(clippy::cast_possible_truncation)]
    pub fn pressure(&self) -> u8 {
        if self.balloon_size == 0 {
            return 100;
        }
        // Equivalent to (201 * balloon - 200 * available) / balloon / 2 for
        // in-range inputs, but evaluated in u128 so the intermediate products
        // cannot overflow. The result is bounded by 100, so the cast is exact.
        let used = self.reserved() as u128;
        let balloon = self.balloon_size as u128;
        ((used * 200 + balloon) / (balloon * 2)) as u8
    }

    pub fn reserved(&self) -> usize {
        self.balloon_size.saturating_sub(self.available_memory)
    }

    pub fn adjusted(&self, target: u8) -> usize {
        if target == 0 {
            return self.balloon_size;
        }
        self.reserved().saturating_mul(100) / target as usize
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

async fn monitor_qemu(args: &Args) -> Result<()> {
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

async fn monitor_crosvm(args: &Args) -> Result<()> {
    if args.maximum == usize::MAX {
        bail!("--maximum is required for the crosvm backend");
    }

    let mut endpoints: HashMap<_, Option<Instant>> = args
        .socket
        .iter()
        .map(|path| {
            (
                CrosvmEndpoint::new(&args.crosvm_binary, path),
                None::<Instant>,
            )
        })
        .collect();
    let dur = Duration::from_secs(args.interval);
    let bival = Duration::from_secs(args.balloon_interval);
    let mut ival = tokio::time::interval(dur);
    let mut errors = 0;
    ival.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ival.tick().await;
        for (endpoint, last_balloon) in &mut endpoints {
            let result = async {
                let balloon = endpoint.query_balloon().await?;
                let visible_memory = args.maximum.saturating_sub(balloon.balloon_actual);
                let stats = MemoryStats {
                    balloon_size: visible_memory,
                    base_memory: args.maximum,
                    plugged_memory: 0,
                    total_memory: args.maximum,
                    free_memory: balloon.free_memory,
                    available_memory: balloon.available_memory,
                };

                debug!(
                    "Stats for {endpoint}: {stats}, pressure: {}%, reclaimed: {} MiB",
                    stats.pressure(),
                    balloon.balloon_actual / 1024 / 1024
                );
                if let Some(target) = stats
                    .window(args.low, args.high)
                    .map(|t| t.clamp(args.minimum, args.maximum))
                    .filter(|&t| t != stats.balloon_size)
                    .filter(|_| last_balloon.is_none_or(|last| last.elapsed() >= bival))
                {
                    info!(
                        "Adjusting {endpoint} visible memory from {} to {target}",
                        stats.balloon_size
                    );
                    endpoint.set_visible_memory(target, args.maximum).await?;
                    last_balloon.replace(Instant::now());
                }
                Result::Ok(())
            }
            .await;

            if let Err(error) = result {
                errors += 1;
                if errors >= 5 {
                    return Err(error);
                }
                warn!("Got error {error} with {endpoint} for the {errors}th time");
            } else {
                errors = 0;
            }
        }
    }
}

async fn monitor_memory(args: Args) -> Result<()> {
    if args.minimum > args.maximum {
        bail!("--minimum cannot exceed --maximum");
    }

    match args.hypervisor {
        Hypervisor::Crosvm => monitor_crosvm(&args).await,
        Hypervisor::Qemu => monitor_qemu(&args).await,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    monitor_memory(args).await
}

#[cfg(test)]
mod tests {
    use super::MemoryStats;

    fn stats(balloon_size: usize, available_memory: usize) -> MemoryStats {
        MemoryStats {
            balloon_size,
            base_memory: 0,
            plugged_memory: 0,
            total_memory: 0,
            free_memory: 0,
            available_memory,
        }
    }

    #[test]
    fn pressure_matches_the_original_formula_for_sane_input() {
        for (balloon, available) in [
            (1024_usize, 512_usize),
            (4096, 4000),
            (8192, 1),
            (1000, 999),
        ] {
            let expected = ((201 * balloon - 200 * available) / balloon / 2) as u8;
            assert_eq!(stats(balloon, available).pressure(), expected);
        }
    }

    #[test]
    fn pressure_is_bounded_and_never_panics_on_guest_reported_values() {
        // available > balloon: used to underflow and wrap in release builds.
        assert_eq!(stats(1024, 2048).pressure(), 0);
        assert_eq!(stats(1024, usize::MAX).pressure(), 0);
        // A zero balloon used to divide by zero.
        assert_eq!(stats(0, 0).pressure(), 100);
        assert_eq!(stats(0, usize::MAX).pressure(), 100);
        // No input can push the value past 100.
        assert_eq!(stats(usize::MAX, 0).pressure(), 100);
    }

    #[test]
    fn reserved_and_adjusted_saturate() {
        assert_eq!(stats(1024, 2048).reserved(), 0);
        assert_eq!(stats(2048, 1024).reserved(), 1024);
        // A zero target would otherwise divide by zero.
        assert_eq!(stats(2048, 1024).adjusted(0), 2048);
        assert_eq!(stats(2048, 1024).adjusted(50), 2048);
        // The multiplication cannot overflow.
        assert_eq!(stats(usize::MAX, 0).adjusted(100), usize::MAX / 100);
    }
}
