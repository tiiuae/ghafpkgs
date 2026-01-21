/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, result::Result as StdResult, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufStream},
    net::UnixStream,
    sync::mpsc,
    time::{Sleep, sleep},
};

pub type Result<T> = anyhow::Result<T>;

const TIMEOUT_SEC: u64 = 3;
const TIMEOUT: Duration = Duration::from_secs(TIMEOUT_SEC);

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(tag = "execute", content = "arguments", rename_all = "kebab-case")]
pub enum Command {
    #[serde(rename = "qmp_capabilities")]
    QmpCapabilities,
    QueryBalloon,
    Balloon {
        value: usize,
    },
    QomSet {
        path: &'static str,
        property: &'static str,
        value: u64,
    },
    QomGet {
        path: &'static str,
        property: &'static str,
    },
    QueryMemorySizeSummary,
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
type CommandChannel = mpsc::Sender<(Command, ReplyChannel)>;

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Endpoint {
    path: PathBuf,
}

pub struct Connection {
    channel: CommandChannel,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
enum QmpResponse {
    Return(serde_json::Value),
    Error(serde_json::Value),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum QmpMessage {
    Response(QmpResponse),
    Event(serde_json::Value),
}

trait QmpStreamExt {
    async fn send_cmd(&mut self, cmd: Command) -> Result<()>;
    async fn get_json(&mut self) -> Result<QmpMessage>;
}

impl<RW: AsyncWrite + AsyncRead + std::marker::Unpin> QmpStreamExt for BufStream<RW> {
    async fn send_cmd(&mut self, cmd: Command) -> Result<()> {
        self.write_all(&serde_json::to_vec(&cmd)?).await?;
        self.write_all(b"\n").await?;
        self.flush().await?;
        Ok(())
    }

    async fn get_json(&mut self) -> Result<QmpMessage> {
        let mut buf = vec![];
        let len = self.read_until(b'\n', &mut buf).await?;
        if len == 0 {
            bail!("Connection closed unexpectedly");
        }

        Ok(serde_json::from_slice(&buf)?)
    }
}

impl Endpoint {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub async fn connect(
        &self,
    ) -> Result<(
        Connection,
        impl std::future::Future<Output = Result<()>>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        Connection::new(
            UnixStream::connect(&self.path)
                .await
                .context("Failed to connect to QMP socket")?,
        )
        .await
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> StdResult<(), std::fmt::Error> {
        self.path.display().fmt(f)
    }
}

impl Connection {
    pub(super) async fn new<S: AsyncRead + AsyncWrite + std::marker::Unpin>(
        stream: S,
    ) -> Result<(
        Connection,
        impl std::future::Future<Output = Result<()>>,
        mpsc::Receiver<serde_json::Value>,
    )> {
        let mut stream = tokio::select! {
            () = sleep(TIMEOUT) => Err(anyhow!("QMP conncetion timed out")),
            r = async {
                let mut stream = BufStream::new(stream);
                stream.get_json().await.context("Handshake failed")?;
                stream.send_cmd(Command::QmpCapabilities).await?;
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
                                    QmpMessage::Response(QmpResponse::Return(r)) => Ok(r),
                                    QmpMessage::Response(QmpResponse::Error(e)) => Err(e),
                                    QmpMessage::Event(e) => {
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
                            stream.send_cmd(cmd).await?;
                            tx.replace((newtx, sleep(TIMEOUT)));
                        },
                        res = async {
                            while let Ok(resp) = stream.get_json().await {
                                let QmpMessage::Event(e) = resp else { continue; };
                                evsender.send(e).await?;
                            }
                            Result::Ok(())
                        } => res?,
                    }
                }
            }
        };

        Ok((Self { channel }, task, evreceiver))
    }

    async fn send_command<T: for<'a> Deserialize<'a>>(&self, cmd: Command) -> Result<T> {
        let (tx, mut rx) = mpsc::channel(1);
        self.channel.send((cmd, tx)).await?;
        Ok(serde_json::from_value(
            rx.recv()
                .await
                .context("Invalid response")?
                .map_err(|e| anyhow!("{e}"))?,
        )?)
    }

    pub async fn query_balloon(&self) -> Result<BalloonInfo> {
        self.send_command(Command::QueryBalloon).await
    }

    pub async fn balloon(&self, value: usize) -> Result<()> {
        let cmd = Command::Balloon { value };
        self.send_command::<Empty>(cmd).await.map(|_| ())
    }

    pub async fn query_memory(&self) -> Result<MemoryInfo> {
        let cmd = Command::QueryMemorySizeSummary;
        self.send_command(cmd).await
    }

    pub async fn set_stats_interval(&self, ival: std::time::Duration) -> Result<()> {
        let cmd = Command::QomSet {
            path: "/machine/peripheral/balloon0",
            property: "guest-stats-polling-interval",
            value: ival.as_secs(),
        };
        self.send_command::<Empty>(cmd).await.map(|_| ())
    }

    pub async fn query_stats(&self) -> Result<GuestMemoryInfo> {
        let cmd = Command::QomGet {
            path: "/machine/peripheral/balloon0",
            property: "guest-stats",
        };
        self.send_command(cmd).await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio::io::AsyncReadExt;

    const TIMEOUT_SLOW: Duration = Duration::from_secs(TIMEOUT_SEC + 1);
    const TIMEOUT_SLOWER: Duration = Duration::from_secs(TIMEOUT_SEC + 2);
    const EVENT_JSON: &[u8] = b"{\"event\":\"EVENT\",\"data\":123}\n";
    const EMPTY_JSON: &[u8] = b"{}\n";
    const ERROR_JSON: &[u8] = b"{\"error\":\"something\"}\n";
    const BALLOON_RETURN_JSON: &[u8] = b"{\"return\":{\"actual\":123}}\n";

    trait IsCmd {
        fn expect_cmd(&self, cmd: &str) -> Result<()>;
    }

    impl IsCmd for serde_json::Value {
        fn expect_cmd(&self, cmd: &str) -> Result<()> {
            self.get("execute")
                .filter(|c| c.as_str() == Some(cmd))
                .map(|_| ())
                .ok_or(anyhow!("Server received invalid command"))
        }
    }

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

    async fn read_json_execute<S: AsyncRead + std::marker::Unpin>(
        stream: &mut S,
        cmd: &str,
    ) -> Result<()> {
        read_json_line(stream).await?.expect_cmd(cmd)
    }

    async fn handshake<S: AsyncRead + AsyncWrite + std::marker::Unpin>(
        stream: &mut S,
    ) -> anyhow::Result<()> {
        match tokio::time::timeout(TIMEOUT_SLOWER, async move {
            stream.write_all(EMPTY_JSON).await?;
            read_json_execute(stream, "qmp_capabilities").await?;
            stream.write_all(EMPTY_JSON).await?;
            Ok(())
        })
        .await
        {
            Ok(r) => r,
            _ => bail!("Handshake timed out"),
        }
    }

    async fn harness(
        fs: impl AsyncFnOnce(tokio::io::DuplexStream) -> anyhow::Result<()>,
        fc: impl AsyncFnOnce(Connection, mpsc::Receiver<serde_json::Value>) -> anyhow::Result<()>,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let (client, mut server) = tokio::io::duplex(4096);
        let (client, task, ev) = tokio::select! {
            e = async {
                handshake(&mut server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = Connection::new(client) => e,
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
    async fn test_connect_timeout() -> anyhow::Result<()> {
        let tmpd = tempfile::tempdir()?;
        let sockpath = tmpd.path().join("socket");
        let _listener = tokio::net::UnixListener::bind(&sockpath)?;
        let qe = Endpoint::new(sockpath);

        tokio::select! {
            e = qe.connect() => {
                if e.is_ok() {
                    bail!("Unexpected connect success");
                }
            },
            _ = tokio::time::sleep(TIMEOUT_SLOW) => {
                bail!("Timed out waiting for timeout");
            },
        };
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_unix_handshake() -> anyhow::Result<()> {
        let tmpd = tempfile::tempdir()?;
        let sockpath = tmpd.path().join("socket");
        let listener = tokio::net::UnixListener::bind(&sockpath)?;
        let qe = Endpoint::new(sockpath);

        tokio::select! {
            e = async move {
                let (mut server, _) = listener.accept().await?;
                handshake(&mut server).await?;
                std::future::pending::<()>().await;
                unreachable!();
            } => e,
            e = qe.connect() => e.map(|_| ()),
            _ = tokio::time::sleep(TIMEOUT_SLOW) => {
                bail!("Timed out waiting for timeout");
            },
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
                match tokio::time::timeout(TIMEOUT_SLOW, Connection::new(client)).await {
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
                match tokio::time::timeout(TIMEOUT_SLOW, Connection::new(client)).await {
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
                match tokio::time::timeout(TIMEOUT_SLOW, Connection::new(client)).await {
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
            async move |mut server| {
                read_json_execute(&mut server, "query-balloon").await?;
                server.write_all(BALLOON_RETURN_JSON).await?;
                Ok(())
            },
            async move |client, mut ev| {
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
            async move |mut server| {
                read_json_execute(&mut server, "query-balloon").await?;
                server.write_all(ERROR_JSON).await?;
                Ok(())
            },
            async move |client, mut ev| {
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
            async move |mut server| read_json_execute(&mut server, "query-balloon").await,
            async move |client, mut ev| {
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
            async move |mut server| {
                server.write_all(EVENT_JSON).await?;
                read_json_execute(&mut server, "query-balloon").await?;
                server.write_all(BALLOON_RETURN_JSON).await?;
                Ok(())
            },
            async move |client, mut ev| {
                let qb = Box::pin(async move {
                    if client.query_balloon().await?.actual != 123 {
                        bail!("Unexpected `actual` value");
                    }
                    Ok(())
                });
                let (ev, qb) = tokio::join! {
                    ev.recv(),
                    qb,
                };
                ev.context("Event not received")?;
                qb
            },
            TIMEOUT_SLOW,
        )
        .await
    }
}
