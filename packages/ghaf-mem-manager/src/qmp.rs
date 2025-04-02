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

pub type Result<T> = anyhow::Result<T>;

const TIMEOUT_SEC: u64 = 3;
const TIMEOUT: Duration = Duration::from_secs(TIMEOUT_SEC);

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
        QmpConnection::new(
            UnixStream::connect(&self.path)
                .await
                .context("Failed to connect to QMP socket")?,
        )
        .await
    }
}

impl std::fmt::Display for QmpEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> StdResult<(), std::fmt::Error> {
        self.path.display().fmt(f)
    }
}

impl QmpConnection {
    pub(super) async fn new<S: AsyncRead + AsyncWrite + std::marker::Unpin>(
        stream: S,
    ) -> Result<(
        QmpConnection,
        impl std::future::Future<Output = Result<()>>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        let mut stream = tokio::select! {
            () = sleep(TIMEOUT) => Err(anyhow!("QMP conncetion timed out")),
            r = async {
                let mut stream = BufStream::new(stream);
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
                            bail!("QMP command timed out");
                        }
                    }
                } else {
                    tokio::select! {
                        cmd = receiver.recv() => {
                            let Some((cmd, newtx)) = cmd else { break Result::Ok(()); };
                            stream.send_cmd(&cmd).await?;
                            tx.replace((newtx, sleep(TIMEOUT)));
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

#[cfg(test)]
mod test {
    use super::*;
    use tokio::io::AsyncReadExt;

    const TIMEOUT_SLOW: Duration = Duration::from_secs(TIMEOUT_SEC + 1);
    const TIMEOUT_SLOWER: Duration = Duration::from_secs(TIMEOUT_SEC + 2);
    const EVENT_JSON: &[u8] = b"{\"event\":{}}\n";
    const EMPTY_JSON: &[u8] = b"{}\n";
    const ERROR_JSON: &[u8] = b"{\"error\":\"something\"}\n";
    const BALLOON_RETURN_JSON: &[u8] = b"{\"return\":{\"actual\":123}}\n";

    async fn read_json_line<S: AsyncRead + std::marker::Unpin>(
        stream: &mut S,
    ) -> anyhow::Result<serde_json::Value> {
        let mut buf = Vec::new();
        loop {
            let c = stream.read_u8().await?;
            if c == b'\n' {
                break Ok(serde_json::from_slice(&buf)?);
            }

            buf.push(c);
        }
    }

    async fn handshake<S: AsyncRead + AsyncWrite + std::marker::Unpin>(
        stream: &mut S,
    ) -> anyhow::Result<()> {
        match tokio::time::timeout(TIMEOUT_SLOWER, async move {
            stream.write_all(EMPTY_JSON).await?;
            read_json_line(stream).await?;
            stream.write_all(EMPTY_JSON).await?;
            Ok(())
        })
        .await
        {
            Ok(r) => r,
            _ => bail!("Handshake timed out"),
        }
    }

    async fn harness<
        FuS: std::future::Future<Output = anyhow::Result<()>>,
        FuC: std::future::Future<Output = anyhow::Result<()>>,
        FS: FnOnce(tokio::io::DuplexStream) -> FuS,
        FC: FnOnce(QmpConnection, mpsc::Receiver<serde_json::Value>) -> FuC,
    >(
        fs: FS,
        fc: FC,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let (client, mut server) = tokio::io::duplex(4096);
        let (client, task, ev) = tokio::select! {
            e = async {
                handshake(&mut server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = QmpConnection::new(client) => e,
        }?;

        tokio::select! {
            e = async move {
                fs(server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = fc(client, ev) => e,
            _ = task => bail!("Task stopped unexpectedly"),
            _ = tokio::time::sleep(timeout) => bail!("Timed out"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_handshake_timeout() -> anyhow::Result<()> {
        let (client, mut server) = tokio::io::duplex(4096);
        //let mut server = BufStream::new(server);
        tokio::select! {
            e = async move {
                server.write_all(EMPTY_JSON).await?;
                read_json_line(&mut server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = async move {
                match tokio::time::timeout(TIMEOUT_SLOW, QmpConnection::new(client)).await {
                    Err(_) => bail!("Handshake did not time out in {} seconds", TIMEOUT_SLOW.as_secs()),
                    Ok(Ok(_)) => bail!("Handhake succeeded unxepectedly"),
                    _ => Ok(())
                }
            } => e,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_handshake() -> anyhow::Result<()> {
        let (client, mut server) = tokio::io::duplex(4096);
        tokio::select! {
            e = async move {
                handshake(&mut server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = async move {
                match tokio::time::timeout(TIMEOUT_SLOW, QmpConnection::new(client)).await {
                    Err(_) => bail!("Handshake timed out"),
                    Ok(Ok(_)) => Ok(()),
                    _ => bail!("Handshake failed"),
                }
            } => e,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_event() -> anyhow::Result<()> {
        let (client, mut server) = tokio::io::duplex(4096);
        tokio::select! {
            e = async move {
                handshake(&mut server).await?;
                server.write_all(EVENT_JSON).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = async move {
                match tokio::time::timeout(TIMEOUT_SLOW, QmpConnection::new(client)).await {
                    Err(_) => bail!("Handshake timed out"),
                    Ok(Ok(_)) => Ok(()),
                    _ => bail!("Handshake failed"),
                }
            } => e,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_query_command() -> anyhow::Result<()> {
        harness(
            |mut server| async move {
                let serde_json::Value::Object(cmd) = read_json_line(&mut server).await? else {
                    bail!("Unexpected data");
                };
                if cmd
                    .get("execute")
                    .is_none_or(|e| e.as_str() != Some("query-balloon"))
                {
                    bail!("Missing or unexpected command");
                }
                server.write_all(BALLOON_RETURN_JSON).await?;
                Ok(())
            },
            |client, mut ev| async move {
                tokio::select! {
                    _ = ev.recv() => bail!("Unexpected event"),
                    e = async move {
                        if client.query_balloon().await?.actual != 123 {
                            bail!("Unexpceted `actual` value");
                        }
                        Ok(())
                    } => e,
                }
            },
            TIMEOUT_SLOW,
        )
        .await
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_command_error() -> anyhow::Result<()> {
        harness(
            |mut server| async move {
                let serde_json::Value::Object(cmd) = read_json_line(&mut server).await? else {
                    bail!("Unexpected data");
                };
                if cmd
                    .get("execute")
                    .is_none_or(|e| e.as_str() != Some("query-balloon"))
                {
                    bail!("Missing or unexpected command");
                }
                server.write_all(ERROR_JSON).await?;
                Ok(())
            },
            |client, mut ev| async move {
                tokio::select! {
                    _ = ev.recv() => bail!("Unexpected event"),
                    r = async move {
                        if client.query_balloon().await.is_ok() {
                            bail!("Unexpected success");
                        }
                        Ok(())
                    } => r,
                }
            },
            TIMEOUT_SLOW,
        )
        .await
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_command_timeout() -> anyhow::Result<()> {
        harness(
            |mut server| async move {
                let serde_json::Value::Object(cmd) = read_json_line(&mut server).await? else {
                    bail!("Unexpected data");
                };
                if cmd
                    .get("execute")
                    .is_none_or(|e| e.as_str() != Some("query-balloon"))
                {
                    bail!("Missing or unexpected command");
                }
                Ok(())
            },
            |client, mut ev| async move {
                tokio::select! {
                    _ = ev.recv() => bail!("Unexpected event"),
                    e = async move {
                        client.query_balloon().await.map(|_| ())
                    } => e,
                }
            },
            TIMEOUT_SLOW,
        )
        .await
        .err()
        .map(|_| ())
        .context("Unexpected success")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_query_command_with_event() -> anyhow::Result<()> {
        harness(
            |mut server| async move {
                server.write_all(EVENT_JSON).await?;
                let serde_json::Value::Object(cmd) = read_json_line(&mut server).await? else {
                    bail!("Unexpected data");
                };
                if cmd
                    .get("execute")
                    .is_none_or(|e| e.as_str() != Some("query-balloon"))
                {
                    bail!("Missing or unexpected command");
                }
                tokio::task::yield_now().await;
                server.write_all(BALLOON_RETURN_JSON).await?;
                Ok(())
            },
            |client, mut ev| async move {
                let mut qb = Box::pin(async move {
                    if client.query_balloon().await?.actual != 123 {
                        bail!("Unexpected `actual` value");
                    }
                    Ok(())
                });
                tokio::select! {
                    _ = ev.recv() => Ok::<_, anyhow::Error>(()),
                    _ = &mut qb => bail!("Command completed before event"),
                }?;
                qb.await
            },
            TIMEOUT_SLOW,
        )
        .await
    }
}
