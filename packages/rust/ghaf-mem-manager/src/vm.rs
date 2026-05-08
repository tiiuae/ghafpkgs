/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use std::{path::PathBuf, sync::Arc};

pub use anyhow::Error;
use anyhow::Result;
use tokio::sync::{Mutex, mpsc};
use tracing::debug;

use crate::BYTES_IN_MIB;
use crate::qmp;

const GUEST_STATS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Debug)]
struct MemoryStats {
    pub last_update: u64,
    balloon_size: u64,
    base_memory: u64,
    plugged_memory: u64,
    total_memory: u64,
    free_memory: u64,
    available_memory: u64,
}

impl MemoryStats {
    pub fn new(
        mem_info: &qmp::MemoryInfo,
        guest_info: &qmp::GuestMemoryInfo,
        bal_info: &qmp::BalloonInfo,
    ) -> Option<Self> {
        let available_memory = guest_info.stats.stat_available_memory;
        let free_memory = guest_info.stats.stat_free_memory;

        if guest_info.stats.stat_available_memory == u64::MAX
            || bal_info.actual == 0
            || bal_info.actual < available_memory
        {
            debug!("Got invalid data: {bal_info:?} {guest_info:?}");
            return None;
        }

        Some(Self {
            last_update: guest_info.last_update,
            balloon_size: bal_info.actual,
            base_memory: mem_info.base_memory,
            plugged_memory: mem_info.plugged_memory,
            total_memory: mem_info.base_memory + mem_info.plugged_memory,
            free_memory,
            available_memory,
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn pressure(&self) -> u8 {
        ((201 * self.balloon_size - 200 * self.available_memory) / self.balloon_size / 2) as u8
    }

    pub fn reserved(&self) -> u64 {
        self.balloon_size - self.available_memory
    }

    pub fn adjusted(&self, target: u8) -> u64 {
        self.reserved() * 100 / u64::from(target)
    }

    pub fn window(&self, min: u8, max: u8) -> Option<u64> {
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
            self.balloon_size / BYTES_IN_MIB,
            self.base_memory / BYTES_IN_MIB,
            self.plugged_memory / BYTES_IN_MIB,
            self.total_memory / BYTES_IN_MIB,
            self.free_memory / BYTES_IN_MIB,
            self.available_memory / BYTES_IN_MIB,
        )
    }
}

struct Session {
    conn: qmp::Connection,
    joinset: tokio::task::JoinSet<qmp::Result<()>>,
}

#[derive(Default)]
struct VMState {
    session: Option<Session>,
    last_update: Option<(u64, u64)>,
}

pub(crate) struct VM {
    endpoint: qmp::Endpoint,
    minimum: u64,
    maximum: u64,
    state: Arc<Mutex<VMState>>,
    manage_trigger: mpsc::UnboundedSender<()>,
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
    pub current: u64,
    pub preferred: u64,
    pub observed_pressure: Option<u8>,
}

impl From<&MemoryStats> for MemInfo {
    fn from(other: &MemoryStats) -> Self {
        Self {
            current: other.balloon_size,
            preferred: other.balloon_size,
            observed_pressure: Some(other.pressure()),
        }
    }
}

impl VM {
    pub fn new(
        endpoint: impl Into<PathBuf>,
        minimum: u64,
        maximum: u64,
        manage_trigger: mpsc::UnboundedSender<()>,
    ) -> Self {
        Self {
            endpoint: qmp::Endpoint::new(endpoint),
            minimum,
            maximum,
            state: Arc::new(Mutex::new(VMState::default())),
            manage_trigger,
        }
    }

    fn clamp(&self, size: u64) -> u64 {
        size.clamp(self.minimum, self.maximum)
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn scale_preferred(&self, preferred: u64, scale: f32) -> u64 {
        ((preferred - self.minimum) as f32 * scale) as u64 + self.minimum
    }

    pub fn minimum(&self) -> u64 {
        self.minimum
    }

    fn event_watcher(
        &self,
        mut events: mpsc::Receiver<serde_json::Value>,
    ) -> impl Future<Output = qmp::Result<()>> + use<> {
        let endpoint = self.endpoint.clone();
        let manage_trigger = self.manage_trigger.clone();

        async move {
            while let Some(event) = events.recv().await {
                let Ok(qmp::Event::BalloonChange {
                    data: qmp::BalloonChange { .. },
                    ..
                }) = serde_json::from_value(event)
                else {
                    continue;
                };

                debug!("Detected BALLOON_CHANGE on {endpoint}, scheduling rebalance");
                let _ = manage_trigger.send(());
            }
            Ok(())
        }
    }

    async fn ensure_connection(&self) -> Result<qmp::Connection, Error> {
        let mut state = self.state.lock().await;
        state
            .session
            .take_if(|session| session.joinset.try_join_next().is_some());

        let conn = if let Some(session) = state.session.as_ref() {
            session.conn.clone()
        } else {
            let (conn, task, events) = self.endpoint.connect().await?;
            let mut joinset = tokio::task::JoinSet::new();
            joinset.spawn(task);
            joinset.spawn(self.event_watcher(events));
            conn.set_stats_interval(GUEST_STATS_POLL_INTERVAL).await?;
            *state = VMState {
                session: Some(Session {
                    conn: conn.clone(),
                    joinset,
                }),
                ..VMState::default()
            };
            conn
        };

        Ok(conn)
    }

    async fn drop_connection(&self) {
        let mut state = self.state.lock().await;
        state.session = None;
    }

    async fn calc_preferred(
        &self,
        conn: &qmp::Connection,
        low: u8,
        high: u8,
    ) -> Result<MemInfo, Error> {
        let balloon = conn.query_balloon().await?;
        let memory = conn.query_memory().await?;
        let guest_stats = conn.query_stats().await?;

        let mut state = self.state.lock().await;
        let mem_stats = MemoryStats::new(&memory, &guest_stats, &balloon);

        // Sannity check of guest data failed; report current balloon state
        let Some(mem_stats) = mem_stats else {
            return Ok(MemInfo {
                current: balloon.actual,
                preferred: balloon.actual,
                observed_pressure: None,
            });
        };

        let last = state
            .last_update
            .replace((mem_stats.last_update, balloon.actual));

        Ok(MemInfo {
            preferred: mem_stats
                .window(low, high)
                .filter(|_| last.is_none_or(|(ts, _)| ts < mem_stats.last_update))
                .map_or(mem_stats.balloon_size, |sz| self.clamp(sz)),
            ..MemInfo::from(&mem_stats)
        })
    }

    pub async fn adjust(&self, balloon: u64) -> Result<(), Error> {
        let conn = self.ensure_connection().await?;

        match conn.balloon(balloon).await {
            Err(e) => {
                self.drop_connection().await;
                Err(e)
            }
            Ok(()) => Ok(()),
        }
    }

    pub async fn preferred_memory_size(&self, low: u8, high: u8) -> Result<MemInfo, Error> {
        let conn = self.ensure_connection().await?;
        match self.calc_preferred(&conn, low, high).await {
            Ok(info) => Ok(info),
            Err(e) => {
                self.drop_connection().await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{Result, anyhow, bail};
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
        sync::mpsc,
        task::JoinHandle,
        time::{Duration, timeout},
    };

    const GREETING_JSON: &[u8] = b"{\"QMP\":{\"version\":{\"qemu\":{\"major\":8,\"minor\":2,\"micro\":0},\"package\":\"\"},\"capabilities\":[]}}\n";
    const EMPTY_RETURN_JSON: &[u8] = b"{\"return\":{}}\n";

    async fn read_json_line<S: AsyncRead + std::marker::Unpin>(stream: &mut S) -> Result<Value> {
        let mut buf = Vec::new();
        loop {
            let c = stream.read_u8().await?;
            if c == b'\n' {
                break Ok(serde_json::from_slice(&buf)?);
            }
            buf.push(c);
        }
    }

    async fn write_json_line<S: AsyncWrite + std::marker::Unpin>(
        stream: &mut S,
        value: &Value,
    ) -> Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn expect_execute<S: AsyncRead + std::marker::Unpin>(
        stream: &mut S,
        cmd: &str,
    ) -> Result<Value> {
        let req = read_json_line(stream).await?;
        let got = req
            .get("execute")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("request missing execute"))?;
        if got != cmd {
            bail!("unexpected command: expected {cmd}, got {got}");
        }
        Ok(req)
    }

    async fn mock_connection_for_calc_preferred(
        balloon_actual: usize,
        base_memory: usize,
        plugged_memory: usize,
        last_update: u64,
        stat_available_memory: u64,
        stat_free_memory: u64,
    ) -> Result<(
        qmp::Connection,
        JoinHandle<qmp::Result<()>>,
        JoinHandle<Result<()>>,
    )> {
        let (client, mut server) = tokio::io::duplex(4096);
        let server_task = tokio::spawn(async move {
            server.write_all(GREETING_JSON).await?;
            server.flush().await?;
            expect_execute(&mut server, "qmp_capabilities").await?;
            server.write_all(EMPTY_RETURN_JSON).await?;
            server.flush().await?;

            expect_execute(&mut server, "query-balloon").await?;
            write_json_line(&mut server, &json!({"return": {"actual": balloon_actual}})).await?;

            expect_execute(&mut server, "query-memory-size-summary").await?;
            write_json_line(
                &mut server,
                &json!({
                    "return": {
                        "base-memory": base_memory,
                        "plugged-memory": plugged_memory
                    }
                }),
            )
            .await?;

            let qom_get = expect_execute(&mut server, "qom-get").await?;
            let property = qom_get
                .get("arguments")
                .and_then(|a| a.get("property"))
                .and_then(Value::as_str);
            if property != Some("guest-stats") {
                bail!("unexpected qom-get property: {property:?}");
            }
            write_json_line(
                &mut server,
                &json!({
                    "return": {
                        "last-update": last_update,
                        "stats": {
                            "stat-available-memory": stat_available_memory,
                            "stat-free-memory": stat_free_memory
                        }
                    }
                }),
            )
            .await?;

            Ok(())
        });

        let (conn, task, _events) = qmp::Connection::new(client).await?;
        Ok((conn, tokio::spawn(task), server_task))
    }

    #[tokio::test]
    async fn calc_preferred_invalid_stats_without_oom_keeps_balloon() -> Result<()> {
        let (manage_tx, _manage_rx) = mpsc::unbounded_channel();
        let vm = VM::new("/tmp/test-vm.sock", 0, u64::MAX, manage_tx);
        let (conn, qmp_task, server_task) =
            mock_connection_for_calc_preferred(830, 1024, 0, 1, u64::MAX, 0).await?;

        let info = vm.calc_preferred(&conn, 75, 85).await?;
        assert_eq!(info.current, 830);
        assert_eq!(info.preferred, 830);

        let _ = server_task.await?;
        qmp_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn calc_preferred_valid_stats_adjusts_target() -> Result<()> {
        let (manage_tx, _manage_rx) = mpsc::unbounded_channel();
        let vm = VM::new("/tmp/test-vm.sock", 0, u64::MAX, manage_tx);
        let (conn, qmp_task, server_task) =
            mock_connection_for_calc_preferred(800, 1024, 0, 7, 400, 200).await?;

        let info = vm.calc_preferred(&conn, 75, 85).await?;
        assert_eq!(info.current, 800);
        assert_eq!(info.preferred, 533);

        let _ = server_task.await?;
        qmp_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn calc_preferred_stale_valid_stats_do_not_override_balloon_change() -> Result<()> {
        let (manage_tx, _manage_rx) = mpsc::unbounded_channel();
        let vm = VM::new("/tmp/test-vm.sock", 0, u64::MAX, manage_tx);
        {
            let mut state = vm.state.lock().await;
            state.last_update = Some((7, 700));
        }
        let (conn, qmp_task, server_task) =
            mock_connection_for_calc_preferred(900, 1024, 0, 7, 450, 200).await?;

        let info = vm.calc_preferred(&conn, 75, 85).await?;
        assert_eq!(info.current, 900);
        assert_eq!(info.preferred, 900);

        let state = vm.state.lock().await;
        assert_eq!(state.last_update, Some((7, 900)));
        drop(state);

        let _ = server_task.await?;
        qmp_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn balloon_change_event_triggers_manage() -> Result<()> {
        let (manage_tx, mut manage_rx) = mpsc::unbounded_channel();
        let mut joinset = tokio::task::JoinSet::new();
        let vm = VM::new("/tmp/test-vm.sock", 0, u64::MAX, manage_tx);

        let (event_tx, event_rx) = mpsc::channel(1);
        joinset.spawn(vm.event_watcher(event_rx));
        event_tx
            .send(json!({
                "event":"BALLOON_CHANGE",
                "data":{"actual":900},
                "timestamp":{"seconds":42,"microseconds":123456}
            }))
            .await?;

        timeout(Duration::from_secs(1), manage_rx.recv())
            .await
            .map_err(|_| anyhow!("timed out waiting for manage trigger"))?
            .ok_or_else(|| anyhow!("manage trigger channel unexpectedly closed"))?;

        Ok(())
    }
}
