/*
 * Copyright 2025 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufStream},
    net::UnixStream,
    sync::mpsc,
    time::{sleep, Sleep},
};
use tracing::info;

#[derive(Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct QmpCommand {
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

    pub fn arg<T: Into<serde_json::Value>>(mut self, key: &'static str, v: T) -> Self {
        self.arguments.insert(key, v.into());
        self
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct BalloonInfo {
    pub actual: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct MemoryInfo {
    pub base_memory: usize,
    pub plugged_memory: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct GuestMemoryStats {
    pub stat_available_memory: usize,
    pub stat_free_memory: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct GuestMemoryInfo {
    pub last_update: usize,
    pub stats: GuestMemoryStats,
}

#[derive(Deserialize, Debug)]
struct Empty {}

type ReplyChannel = mpsc::Sender<Result<serde_json::Value, serde_json::Value>>;
type CommandChannel = mpsc::Sender<(QmpCommand, ReplyChannel)>;

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct QmpEndpoint {
    path: PathBuf,
}

pub struct QmpConnection {
    channel: CommandChannel,
}

impl QmpEndpoint {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub async fn connect(
        &self,
    ) -> Result<(
        QmpConnection,
        impl std::future::Future<Output = ()>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        let mut buf = vec![];
        let mut stream = tokio::select! {
            _ = sleep(Duration::from_secs(3)) => Err(anyhow!("QMP conncetion timed out")),
            r = async {
                let mut stream = BufStream::new(
                    UnixStream::connect(&self.path)
                    .await
                    .context("Failed to connect to QMP socket")?,
                );
                info!("Connected to {}", self.path.display());
                stream.read_until(b'\n', &mut buf).await?;
                buf.clear();
                stream
                    .write_all(&serde_json::to_vec(&QmpCommand::new("qmp_capabilities"))?)
                    .await?;
                stream.write_all(b"\n").await?;
                stream.flush().await?;
                stream.read_until(b'\n', &mut buf).await?;
                Ok(stream)
            } => r
        }?;

        let (channel, mut receiver) = mpsc::channel(16);
        let (evsender, evreceiver) = mpsc::channel(16);
        let mut tx: Option<(ReplyChannel, Sleep)> = None;

        let task = async move {
            loop {
                if let Some((curtx, timeout)) = tx.take() {
                    buf.clear();
                    tokio::select! {
                        _ = async {
                            while let Ok(len) = stream.read_until(b'\n', &mut buf).await {
                                if len == 0 {
                                    return;
                                }
                                let Ok(serde_json::Value::Object(mut data)) = serde_json::from_slice(&buf)
                                else {
                                    continue;
                                };
                                if let Some(reply) = data.remove("return") {
                                    let _ = curtx.send(Ok(reply)).await;
                                    break;
                                } else if let Some(error) = data.remove("error") {
                                    let _ = curtx.send(Err(error)).await;
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
                        } => {},
                        _ = timeout => {
                            let _ = curtx.send(Err("Command timed out".into())).await;
                            break;
                        }
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
                                tx = Some((newtx, sleep(Duration::from_secs(3))));
                            } else {
                                let _ = newtx.send(Err("Command serialization failed".into())).await;
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

        Ok((QmpConnection { channel }, task, evreceiver))
    }
}

impl std::fmt::Display for QmpEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.path.display())
    }
}

impl QmpConnection {
    async fn send_command<T: for<'a> Deserialize<'a>>(&self, cmd: QmpCommand) -> Result<T> {
        let (tx, mut rx) = mpsc::channel(1);
        self.channel.send((cmd, tx)).await?;
        Ok(serde_json::from_value(
            rx.recv()
                .await
                .context("Invalid response")?
                .map_err(|e| anyhow!("{}", e.to_string()))?,
        )?)
    }

    pub async fn query_balloon(&self) -> Result<BalloonInfo> {
        let cmd = QmpCommand::new("query-balloon");
        self.send_command(cmd).await
    }

    pub async fn balloon(&self, size: usize) -> Result<()> {
        let cmd = QmpCommand::new("balloon").arg("value", size);
        self.send_command(cmd).await
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
        self.send_command(cmd).await
    }

    pub async fn query_stats(&self) -> Result<GuestMemoryInfo> {
        let cmd = QmpCommand::new("qom-get")
            .arg("path", "/machine/peripheral/balloon0")
            .arg("property", "guest-stats");
        self.send_command(cmd).await
    }
}
