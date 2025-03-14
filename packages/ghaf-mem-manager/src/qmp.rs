/*
 * Copyright 2025 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use anyhow::{anyhow, bail, Context};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, result::Result as StdResult, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufStream},
    net::UnixStream,
    sync::mpsc,
    time::{sleep, Sleep},
};
use tracing::trace;

pub type Result<T> = anyhow::Result<T>;

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

type ReplyChannel = mpsc::Sender<StdResult<serde_json::Value, serde_json::Value>>;
type CommandChannel = mpsc::Sender<(QmpCommand, ReplyChannel)>;

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct QmpEndpoint {
    path: PathBuf,
}

pub struct QmpConnection {
    channel: CommandChannel,
}

enum QmpResponse {
    Return(serde_json::Value),
    Error(serde_json::Value),
    Event(serde_json::Value),
}

trait QmpStreamExt {
    async fn send_cmd(&mut self, cmd: &QmpCommand) -> Result<()>;
    async fn get_json(&mut self) -> Result<QmpResponse>;
}

impl<RW: AsyncWrite + AsyncRead + std::marker::Unpin> QmpStreamExt for BufStream<RW> {
    async fn send_cmd(&mut self, cmd: &QmpCommand) -> Result<()> {
        self.write_all(&serde_json::to_vec(cmd)?).await?;
        self.write_all(b"\n").await?;
        self.flush().await?;
        Ok(())
    }

    async fn get_json(&mut self) -> Result<QmpResponse> {
        let mut buf = vec![];
        let len = self.read_until(b'\n', &mut buf).await?;
        if len == 0 {
            bail!("Connection closed unexpectedly");
        }
        let serde_json::Value::Object(mut data) = serde_json::from_slice(&buf)? else {
            bail!("Unexpceted reply type");
        };

        if let Some(ret) = data.remove("return") {
            Ok(QmpResponse::Return(ret))
        } else if let Some(err) = data.remove("error") {
            Ok(QmpResponse::Error(err))
        } else {
            Ok(QmpResponse::Event(data.into()))
        }
    }
}

impl QmpEndpoint {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub async fn connect(
        &self,
    ) -> Result<(
        QmpConnection,
        impl std::future::Future<Output = Result<()>>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        let mut stream = tokio::select! {
            () = sleep(Duration::from_secs(3)) => Err(anyhow!("QMP conncetion timed out")),
            r = async {
                let mut stream = BufStream::new(
                    UnixStream::connect(&self.path)
                    .await
                    .context("Failed to connect to QMP socket")?,
                );
                trace!("Connected to {self}");
                stream.get_json().await.context("Handshake failed")?;
                stream.send_cmd(&QmpCommand::new("qmp_capabilities")).await?;
                stream.get_json().await.context("Capabilities query failed")?;
                Ok(stream)
            } => r
        }?;

        let (channel, mut receiver) = mpsc::channel(16);
        let (evsender, evreceiver) = mpsc::channel(16);
        let mut tx: Option<(ReplyChannel, Sleep)> = None;

        let task = async move {
            loop {
                if let Some((curtx, timeout)) = tx.take() {
                    tokio::select! {
                        e = async {
                            loop {
                                let reply = match stream.get_json().await? {
                                    QmpResponse::Return(r) => Ok(r),
                                    QmpResponse::Error(e) => Err(e),
                                    QmpResponse::Event(e) => {
                                        evsender.send(e).await?;
                                        continue;
                                    },
                                };

                                break curtx.send(reply).await.map_err(anyhow::Error::from);
                            }
                        } => e?,
                        () = timeout => {
                            bail!("QMP connection timed out");
                        }
                    }
                } else {
                    tokio::select! {
                        cmd = receiver.recv() => {
                            let Some((cmd, newtx)) = cmd else { break Result::Ok(()); };
                            stream.send_cmd(&cmd).await?;
                            tx.replace((newtx, sleep(Duration::from_secs(3))));
                        },
                        res = async {
                            while let Ok(resp) = stream.get_json().await {
                                let QmpResponse::Event(e) = resp else { continue; };
                                evsender.send(e).await?;
                            }
                            Result::Ok(())
                        } => res?,
                    }
                }
            }
        };

        Ok((QmpConnection { channel }, task, evreceiver))
    }
}

impl std::fmt::Display for QmpEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> StdResult<(), std::fmt::Error> {
        self.path.display().fmt(f)
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
        self.send_command::<Empty>(cmd).await.map(|_| ())
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
