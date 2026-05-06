/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::Result;
use clap::Parser;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{RwLock, mpsc};
use tokio::time::Instant;
use tracing::{debug, trace, warn};

mod qmp;
mod vm;

pub const BYTES_IN_MIB: u64 = 1024 * 1024;

#[derive(Default)]
struct VMData {
    errors: usize,
}

#[derive(Clone)]
struct MemManager {
    low: u8,
    high: u8,
    machines: Arc<RwLock<HashMap<vm::VM, VMData>>>,
    manage_trigger: mpsc::UnboundedSender<()>,
}

fn host_available_memory() -> Result<u64> {
    use std::io::BufRead;
    let mi = std::io::BufReader::new(
        std::fs::OpenOptions::new()
            .read(true)
            .open("/proc/meminfo")?,
    );
    for l in mi.lines() {
        if let Some(avail) = l?.strip_prefix("MemAvailable:")
            && let Some(avail) = avail.split_ascii_whitespace().next()
        {
            return avail.parse().map(|kb: u64| kb * 1024).map_err(Into::into);
        }
    }
    Err(anyhow::anyhow!("Unable to read available memory"))
}

impl MemManager {
    fn new(low: u8, high: u8, manage_trigger: mpsc::UnboundedSender<()>) -> Self {
        Self {
            machines: Arc::new(RwLock::new(HashMap::new())),
            low,
            high,
            manage_trigger,
        }
    }

    async fn manage(&self) -> Result<()> {
        let mut mem = host_available_memory()?;
        let mut reqd = 0;
        let mut min = 0;
        let mut machines = self.machines.write().await;
        let mut infos = Vec::new();
        let mut err = false;

        for (m, data) in machines.iter_mut() {
            min += m.minimum();
            if let Ok(info) = m.preferred_memory_size(self.low, self.high).await {
                mem += info.current;
                reqd += info.preferred;
                infos.push(info);
                data.errors = 0;
            } else {
                data.errors += 1;
                err |= true;
            }
        }

        if err {
            for (m, _) in machines.extract_if(|_, d| d.errors >= 5) {
                warn!("Connecting to {m} failed 5 times, dropping");
            }
            anyhow::bail!("Failed to connect to some VMs, retrying later");
        }
        let machines = machines.downgrade();

        let scale = balance_scale(mem, reqd, min);
        if reqd > mem {
            warn!("Memory limited, VMs requested {reqd}, available {mem}");
        }
        if reqd > mem && mem <= min {
            warn!("Available memory {mem} is at or below configured VM minimum total {min}");
        }

        let rounds: Vec<_> = machines
            .keys()
            .zip(infos)
            .filter_map(|(machine, info)| {
                let adjusted = machine.scale_preferred(info.preferred, scale);
                (adjusted != info.current).then_some((machine, info, adjusted))
            })
            .collect();

        for (machine, info, adjusted) in rounds {
            let observed = info.observed_pressure.unwrap_or(u8::MAX);

            debug!(
                "adjust {machine} pressure={observed} size={from_mi}Mi->{adjusted}Mi range={low}-{high}",
                from_mi = info.current / BYTES_IN_MIB,
                adjusted = adjusted / BYTES_IN_MIB,
                low = self.low,
                high = self.high,
            );
            let _ = machine.adjust(adjusted).await;
        }

        Ok(())
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn balance_scale(available: u64, requested: u64, minimum_total: u64) -> f32 {
    if requested <= available {
        1.0
    } else if requested == minimum_total || available <= minimum_total {
        0.0
    } else {
        (available - minimum_total) as f32 / (requested - minimum_total) as f32
    }
}

#[zbus::interface(name = "ae.tii.MemManager", spawn = false)]
impl MemManager {
    async fn attach_vm(
        &self,
        socket: PathBuf,
        minimum: u64,
        maximum: u64,
    ) -> Result<(), zbus::fdo::Error> {
        debug!(
            "Attaching to {sock} with memory in range [{minimum}, {maximum}]",
            sock = socket.display()
        );
        self.machines.write().await.insert(
            vm::VM::new(socket, minimum, maximum, self.manage_trigger.clone()),
            VMData::default(),
        );
        Ok(())
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Monitoring interval in seconds
    #[arg(short, long, default_value_t = 3)]
    interval: u64,

    /// Low memory presure
    #[arg(
        short,
        long,
        default_value_t = 75,
        value_parser = clap::value_parser!(u8).range(1..=99)
    )]
    low: u8,

    /// High memory pressure
    #[arg(
        short = 'H',
        long,
        default_value_t = 85,
        value_parser = clap::value_parser!(u8).range(3..=100)
    )]
    high: u8,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    if args.low >= args.high {
        anyhow::bail!(
            "Invalid memory pressure thresholds: --low ({}) must be lower than --high ({})",
            args.low,
            args.high
        );
    }
    let (manage_trigger, mut manage_react_rx) = mpsc::unbounded_channel();
    let manager = MemManager::new(args.low, args.high, manage_trigger);
    let _conn = zbus::connection::Builder::system()?
        .name("ae.tii.MemManager")?
        .serve_at("/", manager.clone())?
        .build()
        .await?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let periodic_delay = std::time::Duration::from_secs(args.interval);
    let event_delay = std::time::Duration::from_secs(1);
    let start = Instant::now() + periodic_delay;
    let sleep = tokio::time::sleep_until(start);
    tokio::pin!(sleep);

    tokio::select! {
        _ = sigint.recv() => Ok(()),
        e = async move {
            loop {
                tokio::select! {
                    () = &mut sleep => {}
                    m = manage_react_rx.recv() => {
                        if m.is_none() {
                            anyhow::bail!("Manage trigger channel closed unexpectedly");
                        }

                        while manage_react_rx.try_recv().is_ok() {}
                        sleep.as_mut().reset(Instant::now() + event_delay);
                        continue;
                    }
                }

                if let Err(e) = manager.manage().await {
                    warn!("Got error {e} managing VMs, trying again later");
                }
                sleep.as_mut().reset(Instant::now() + periodic_delay);
            }
        } => e,
    }
}
