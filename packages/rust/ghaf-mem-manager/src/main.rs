/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::Result;
use clap::Parser;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, warn};

mod qmp;
mod vm;

#[derive(Default)]
struct VMData {
    errors: usize,
}

#[derive(Clone)]
struct MemManager {
    low: u8,
    high: u8,
    machines: Arc<RwLock<HashMap<vm::VM, VMData>>>,
}

fn host_available_memory() -> Result<usize> {
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
            return avail.parse().map(|kb: usize| kb * 1024).map_err(Into::into);
        }
    }
    Err(anyhow::anyhow!("Unable to read available memory"))
}

impl MemManager {
    fn new(low: u8, high: u8) -> Self {
        Self {
            machines: Arc::new(RwLock::new(HashMap::new())),
            low,
            high,
        }
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    async fn manage(&self) -> Result<()> {
        let mut mem = host_available_memory()?;
        let mut reqd = 0;
        let mut min = 0;
        let mut machines = self.machines.write().await;
        let mut infos = Vec::new();
        let mut err = false;

        for (m, data) in machines.iter_mut() {
            min += m.minimum;
            if let Ok(info) = m.preferred_memory_size(self.low, self.high).await {
                debug!(
                    "VM {m} with {current} requested {preferred}",
                    current = info.current,
                    preferred = info.preferred,
                );
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

        let scale = if reqd <= mem {
            1.0
        } else if reqd == min {
            0.0
        } else {
            warn!("Memory limited, VMs requested {reqd}, available {mem}");
            (mem - min) as f32 / (reqd - min) as f32
        };

        for (machine, info) in machines.keys().zip(infos) {
            machine
                .adjust(
                    ((info.preferred - machine.minimum) as f32 * scale) as usize + machine.minimum,
                )
                .await
                .ok();
        }

        Ok(())
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
        let Ok(minimum) = usize::try_from(minimum) else {
            return Err(zbus::fdo::Error::InvalidArgs("Minimum out-of-range".into()));
        };
        let Ok(maximum) = usize::try_from(maximum) else {
            return Err(zbus::fdo::Error::InvalidArgs("Maximum out-of-range".into()));
        };
        debug!(
            "Attaching to {sock} with memory in range [{minimum}, {maximum}]",
            sock = socket.display()
        );
        self.machines
            .write()
            .await
            .insert(vm::VM::new(socket, minimum, maximum), VMData::default());
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
    #[arg(short, long, default_value_t = 70)]
    low: u8,

    /// High memory pressure
    #[arg(short = 'H', long, default_value_t = 80)]
    high: u8,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let manager = MemManager::new(args.low, args.high);
    let _conn = zbus::connection::Builder::system()?
        .name("ae.tii.MemManager")?
        .serve_at("/", manager.clone())?
        .build()
        .await?;
    let mut ival = tokio::time::interval(std::time::Duration::from_secs(args.interval));
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    ival.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    tokio::select! {
        _ = sigint.recv() => Ok(()),
        e = async move {
            loop {
                ival.tick().await;

                if let Err(e) = manager.manage().await {
                    warn!("Got error {e} managing VMs, trying again later");
                }
            }
        } => e,
    }
}
