/*
 * Copyright 2025 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufStream},
    net::UnixStream,
    sync::mpsc,
};
use tracing::{info, warn};

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

#[derive(Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct QmpCommand {
    execute: &'static str,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    arguments: HashMap<&'static str, serde_json::Value>,
}

impl QmpCommand {
    pub fn new(cmd: &'static str) -> Self {
        Self {
            execute: cmd,
            arguments: HashMap::new(),
        }
    }

    pub fn arg<T: Into<serde_json::Value>>(self, key: &'static str, v: T) -> Self {
        let Self {
            execute,
            mut arguments,
        } = self;
        arguments.insert(key, v.into());
        Self { execute, arguments }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct BalloonInfo {
    actual: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct MemoryInfo {
    base_memory: usize,
    plugged_memory: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct GuestMemoryStats {
    stat_available_memory: usize,
    stat_free_memory: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct GuestMemoryInfo {
    last_update: usize,
    stats: GuestMemoryStats,
}

#[derive(Deserialize, Debug)]
struct Empty {}

type ReplyChannel = mpsc::Sender<serde_json::Value>;
type CommandChannel = mpsc::Sender<(QmpCommand, ReplyChannel)>;

struct QmpConnection {
    path: PathBuf,
    channel: RefCell<Option<CommandChannel>>,
    last_balloon: RefCell<Instant>,
}

impl QmpConnection {
    fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            path: path.into(),
            channel: RefCell::new(None),
            last_balloon: RefCell::new(Instant::now()),
        }
    }

    async fn connect(
        &self,
    ) -> Result<(
        impl std::future::Future<Output = ()>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        let mut stream = BufStream::new(
            UnixStream::connect(&self.path)
                .await
                .context("Failed to connect to QMP socket")?,
        );
        info!("Connected to {}", self.path.display());
        let mut buf = vec![];
        stream.read_until(b'\n', &mut buf).await?;
        buf.clear();
        stream
            .write_all(&serde_json::to_vec(&QmpCommand::new("qmp_capabilities"))?)
            .await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;
        stream.read_until(b'\n', &mut buf).await?;

        let (sender, mut receiver) = mpsc::channel(16);
        let (evsender, evreceiver) = mpsc::channel(16);
        *self.channel.borrow_mut() = Some(sender);
        let mut tx: Option<ReplyChannel> = None;
        let task = async move {
            loop {
                if let Some(curtx) = tx.take() {
                    buf.clear();
                    while let Ok(len) = stream.read_until(b'\n', &mut buf).await {
                        if len == 0 {
                            return;
                        }
                        let Ok(serde_json::Value::Object(mut data)) = serde_json::from_slice(&buf)
                        else {
                            continue;
                        };
                        if let Some(reply) = data.remove("return") {
                            let _ = curtx.send(reply).await;
                            break;
                        } else if evsender
                            .send(serde_json::Value::Object(data))
                            .await
                            .is_err()
                        {
                            return;
                        }
                        buf.clear();
                    }
                } else {
                    buf.clear();
                    tokio::select! {
                        cmd = receiver.recv() => {
                            let Some((cmd, newtx)) = cmd else { break; };
                            if let Ok(vec) = serde_json::to_vec(&cmd) {
                                if stream.write_all(&vec).await.is_err() ||
                                    stream.write_all(b"\n").await.is_err() ||
                                    stream.flush().await.is_err() {
                                    return;
                                }
                                tx = Some(newtx);
                            } else {
                                warn!("Command serialization failed");
                            }
                        },
                        Ok(len) = stream.read_until(b'\n', &mut buf) => {
                            if len == 0 {
                                return;
                            }
                            let Ok(data) = serde_json::from_slice(&buf) else { continue; };
                            let serde_json::Value::Object(data) = data else { continue; };
                            if evsender.send(serde_json::Value::Object(data)).await.is_err() {
                                return;
                            }
                        },
                    }
                }
            }
        };

        Ok((task, evreceiver))
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.channel.borrow_mut().take();
        Ok(())
    }

    async fn send_command<T: for<'a> Deserialize<'a>>(&self, cmd: QmpCommand) -> Result<T> {
        let (tx, mut rx) = mpsc::channel(1);
        let Some(channel) = self.channel.borrow().as_ref().cloned() else {
            bail!("Not connected");
        };
        channel.send((cmd, tx)).await?;
        Ok(serde_json::from_value(
            rx.recv().await.context("Invalid response")?,
        )?)
    }

    pub async fn query_balloon(&self) -> Result<BalloonInfo> {
        let cmd = QmpCommand::new("query-balloon");
        self.send_command(cmd).await
    }

    pub async fn balloon(&self, size: usize) -> Result<()> {
        let cmd = QmpCommand::new("balloon").arg("value", size);
        self.send_command::<Empty>(cmd)
            .await
            .map(|_| ())
            .inspect(|_| *self.last_balloon.borrow_mut() = Instant::now())
    }

    pub async fn query_memory(&self) -> Result<MemoryInfo> {
        let cmd = QmpCommand::new("query-memory-size-summary");
        self.send_command(cmd).await
    }

    pub async fn set_stats_interval(&self, ival: std::time::Duration) -> Result<()> {
        let cmd = QmpCommand::new("qom-set")
            .arg("path", "/machine/peripheral/balloon0")
            .arg("property", "guest-stats-polling-interval")
            .arg("value", ival.as_secs());
        self.send_command::<Empty>(cmd).await.map(|_| ())
    }

    pub async fn query_stats(&self) -> Result<GuestMemoryInfo> {
        let cmd = QmpCommand::new("qom-get")
            .arg("path", "/machine/peripheral/balloon0")
            .arg("property", "guest-stats");
        self.send_command(cmd).await
    }
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
    pub fn pressure(&self) -> u8 {
        ((self.balloon_size - self.available_memory) as f64 * 100. / self.balloon_size as f64)
            .round() as u8
    }

    pub fn reserved(&self) -> usize {
        self.balloon_size - self.available_memory
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
    let qmps: Vec<_> = args.socket.iter().map(QmpConnection::new).collect();
    let dur = Duration::from_secs(args.interval);
    let mut ival = tokio::time::interval(dur);
    let mut last = None;

    loop {
        ival.tick().await;
        for qmp in &qmps {
            let (task, mut receiver) = match qmp.connect().await {
                Ok(a) => a,
                Err(e) => {
                    warn!(
                        "Connection to {} failed: {e}, trying again later",
                        qmp.path.display()
                    );
                    continue;
                }
            };
            tokio::select! {
                e = async {
                    qmp.set_stats_interval(dur).await?;
                    let balloon = qmp.query_balloon().await?;
                    let memory = qmp.query_memory().await?;
                    let guest_stats = qmp.query_stats().await?;

                    #[allow(clippy::nonminimal_bool)]
                    if !last.is_some_and(|last| last == guest_stats.last_update) {
                        last = Some(guest_stats.last_update);
                        let stats = MemoryStats {
                            balloon_size: balloon.actual,
                            base_memory: memory.base_memory,
                            plugged_memory: memory.plugged_memory,
                            total_memory: memory.base_memory + memory.plugged_memory,
                            free_memory: guest_stats.stats.stat_free_memory,
                            available_memory: guest_stats.stats.stat_available_memory,
                        };

                        let pressure = stats.pressure();
                        if let Some(target) = if pressure < args.low {
                            if qmp.last_balloon.borrow().elapsed().as_secs() > args.balloon_interval {
                                info!("Pressure below limit, inflating balloon");
                                Some(stats.reserved() * 100 / args.low as usize)
                            } else {
                                info!("Pressure below limit, waiting for stabilisation");
                                None
                            }
                        } else if pressure > args.high {
                            if qmp.last_balloon.borrow().elapsed().as_secs() > args.balloon_interval {
                                info!("Pressure above limit, deflating balloon");
                                Some(stats.total_memory.min(stats.reserved() * 100 / (args.high as usize - 2)))
                            } else {
                                info!("Pressure above limit, waiting for stabilisation");
                                None
                            }
                        } else {
                            None
                        } {
                            let target = target.clamp(args.minimum, args.maximum);
                            if target != stats.balloon_size {
                                qmp.balloon(target).await?;
                            }
                        }
                    }

                    qmp.disconnect().await
                } => e,
                _ = task => Ok(()),
                _ = async move {
                    while let Some(e) = receiver.recv().await {
                        info!("Got event: {e:?}");
                    }
                } => Ok(()),
            }?;
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    monitor_memory(args).await
}
