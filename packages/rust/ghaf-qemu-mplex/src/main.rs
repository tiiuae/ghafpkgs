/*
 * SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
*/
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    mem,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(feature = "systemd")]
use std::{
    os::linux::net::SocketAddrExt,
    os::unix::net::{SocketAddr, UnixDatagram},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::{
    io::{
        AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
    },
    net::{UnixListener, UnixStream, unix::OwnedWriteHalf},
    sync::mpsc,
    task::JoinSet,
    time::{Instant, sleep_until, timeout},
};

mod socket_watcher;

use socket_watcher::SocketWatcher;

const CLIENT_CHANNEL_SIZE: usize = 64;
const CLIENT_EVENT_CHANNEL_SIZE: usize = 128;
const COMMAND_QUEUE_SIZE: usize = 256;
const QEMU_EVENT_CHANNEL_SIZE: usize = 256;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum State {
    Waiting,
    Connecting {
        listener: UnixListener,
        connect_worker: JoinSet<()>,
        connect_retry_at: Option<Instant>,
        connect_results_tx: mpsc::UnboundedSender<Result<QemuSession>>,
        connect_results_rx: mpsc::UnboundedReceiver<Result<QemuSession>>,
    },
    Connected {
        listener: UnixListener,
        qemu: QemuSession,
        client_event_tx: mpsc::Sender<ClientEvent>,
        client_event_rx: mpsc::Receiver<ClientEvent>,
    },
}

enum StateRuntimeEvent {
    ConnectResult(Result<QemuSession>),
    ConnectRetryDue,
    Accept(std::io::Result<(UnixStream, tokio::net::unix::SocketAddr)>),
    Incoming(Option<QemuIncoming>),
    ClientEvent(Option<ClientEvent>),
}

impl State {
    fn name(&self) -> &'static str {
        match self {
            State::Waiting => "Waiting",
            State::Connecting { .. } => "Connecting",
            State::Connected { .. } => "Connected",
        }
    }

    fn is_waiting(&self) -> bool {
        matches!(self, State::Waiting)
    }

    fn is_connecting(&self) -> bool {
        matches!(self, State::Connecting { .. })
    }

    fn is_connected(&self) -> bool {
        matches!(self, State::Connected { .. })
    }

    fn qemu_writer(&mut self) -> Result<&mut BufWriter<OwnedWriteHalf>> {
        match self {
            State::Connected {
                qemu: QemuSession { write, .. },
                ..
            } => Ok(write),
            _ => bail!("Not connected to qemu"),
        }
    }

    async fn next_runtime_event(&mut self) -> StateRuntimeEvent {
        match self {
            State::Waiting => std::future::pending::<StateRuntimeEvent>().await,
            State::Connecting {
                connect_worker,
                connect_retry_at,
                connect_results_rx,
                ..
            } => {
                tokio::select! {
                    connect_result = connect_results_rx.recv() => {
                        let result = connect_result.unwrap_or_else(|| {
                            Err(anyhow!("connect result channel closed unexpectedly"))
                        });
                        StateRuntimeEvent::ConnectResult(result)
                    }
                    () = async {
                        if let Some(deadline) = connect_retry_at {
                            sleep_until(*deadline).await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    }, if connect_worker.is_empty() => StateRuntimeEvent::ConnectRetryDue,
                }
            }
            State::Connected {
                listener,
                qemu,
                client_event_rx,
                ..
            } => {
                tokio::select! {
                    accept_result = listener.accept() => StateRuntimeEvent::Accept(accept_result),
                    incoming = qemu.incoming.recv() => StateRuntimeEvent::Incoming(incoming),
                    event = client_event_rx.recv() => StateRuntimeEvent::ClientEvent(event),
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct OobIdWrapper {
    connection: u64,
    id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct IdWrapper {
    id: Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ClientIncoming {
    Execute {
        #[serde(rename = "execute")]
        command: String,
        #[serde(default)]
        arguments: Map<String, Value>,
        id: Option<Value>,
    },
    OobExecute {
        #[serde(rename = "exec-oob")]
        command: String,
        #[serde(default)]
        arguments: Map<String, Value>,
        id: Value,
    },
    Other {
        id: Option<Value>,
    },
}

trait ReplyExt: Sized {
    fn into_id(self) -> Option<Value>;

    fn error_reply(self, desc: &str) -> ClientOutgoing {
        ClientOutgoing::Error {
            data: json!({
                "class": "GenericError",
                "desc": desc,
            }),
            id: self.into_id(),
        }
    }
}

impl ReplyExt for ClientIncoming {
    fn into_id(self) -> Option<Value> {
        match self {
            Self::Execute { id, .. } | Self::Other { id, .. } => id,
            Self::OobExecute { id, .. } => Some(id),
        }
    }
}

impl ReplyExt for QemuOutgoing {
    fn into_id(self) -> Option<Value> {
        match self {
            Self::Execute { id, .. } => id.map(|id| id.id),
            Self::OobExecute {
                id: OobIdWrapper { id, .. },
                ..
            } => Some(id),
        }
    }
}

impl ReplyExt for Value {
    fn into_id(self) -> Option<Value> {
        Some(self)
    }
}

impl ClientIncoming {
    fn id(&self) -> Option<&Value> {
        match self {
            Self::Execute { id, .. } | Self::Other { id, .. } => id.as_ref(),
            Self::OobExecute { id, .. } => Some(id),
        }
    }

    fn command(&self) -> Option<&str> {
        match self {
            Self::Execute { command, .. } | Self::OobExecute { command, .. } => Some(command),
            Self::Other { .. } => None,
        }
    }

    fn is_qmp_capabilities(&self) -> bool {
        self.command() == Some("qmp_capabilities")
    }

    fn into_outgoing(self, client_id: u64) -> Option<QemuOutgoing> {
        match self {
            Self::Execute {
                command,
                arguments,
                id,
            } => Some(QemuOutgoing::Execute {
                command,
                arguments,
                id: id.map(|id| IdWrapper { id }),
            }),
            Self::OobExecute {
                command,
                arguments,
                id,
            } => Some(QemuOutgoing::OobExecute {
                command,
                arguments,
                id: OobIdWrapper {
                    connection: client_id,
                    id,
                },
            }),
            Self::Other { .. } => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ClientOutgoing {
    Return {
        #[serde(rename = "return")]
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<Value>,
    },
    Error {
        #[serde(rename = "error")]
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<Value>,
    },
    Event {
        #[serde(flatten)]
        data: Value,
    },
}

impl ClientOutgoing {
    fn id(&self) -> Option<&Value> {
        match self {
            Self::Return { id, .. } | Self::Error { id, .. } => id.as_ref(),
            Self::Event { .. } => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum QemuOutgoing {
    Execute {
        #[serde(rename = "execute")]
        command: String,
        #[serde(skip_serializing_if = "Map::is_empty")]
        arguments: Map<String, Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<IdWrapper>,
    },
    OobExecute {
        #[serde(rename = "exec-oob")]
        command: String,
        #[serde(skip_serializing_if = "Map::is_empty")]
        arguments: Map<String, Value>,
        id: OobIdWrapper,
    },
}

impl QemuOutgoing {
    fn is_oob(&self) -> bool {
        matches!(self, &QemuOutgoing::OobExecute { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum QemuIncoming {
    OobReturn {
        id: OobIdWrapper,
        #[serde(rename = "return")]
        data: Value,
    },
    OobError {
        id: OobIdWrapper,
        #[serde(rename = "error")]
        data: Value,
    },
    Return {
        id: Option<IdWrapper>,
        #[serde(rename = "return")]
        data: Value,
    },
    Error {
        id: Option<IdWrapper>,
        #[serde(rename = "error")]
        data: Value,
    },
    Event {
        #[serde(flatten)]
        data: Value,
    },
}

enum Route {
    Client(u64),
    Current,
    Broadcast,
}

impl QemuIncoming {
    fn into_outgoing(self) -> (Route, ClientOutgoing) {
        match self {
            Self::OobReturn {
                id: OobIdWrapper { connection, id },
                data,
            } => (
                Route::Client(connection),
                ClientOutgoing::Return { id: Some(id), data },
            ),
            Self::OobError {
                id: OobIdWrapper { connection, id },
                data,
            } => (
                Route::Client(connection),
                ClientOutgoing::Error { id: Some(id), data },
            ),
            Self::Return { id, data } => (
                Route::Current,
                ClientOutgoing::Return {
                    id: id.map(|id| id.id),
                    data,
                },
            ),
            Self::Error { id, data } => (
                Route::Current,
                ClientOutgoing::Error {
                    id: id.map(|id| id.id),
                    data,
                },
            ),
            Self::Event { data } => (Route::Broadcast, ClientOutgoing::Event { data }),
        }
    }
}

#[derive(Debug)]
enum ClientEvent {
    Message { client_id: u64, payload: Value },
    Disconnected { client_id: u64 },
}

#[derive(Debug)]
struct QueuedCommand {
    client_id: u64,
    payload: QemuOutgoing,
}

struct Client {
    sender: mpsc::Sender<Value>,
    pending_oob: HashSet<Value>,
    handshaken: bool,
}

#[derive(Debug)]
struct QemuSession {
    greeting: Value,
    capabilities: Value,
    write: BufWriter<OwnedWriteHalf>,
    incoming: mpsc::Receiver<QemuIncoming>,
    _reader_tasks: JoinSet<()>,
}

struct Runtime {
    qemu_socket: PathBuf,
    mux_socket: PathBuf,
    state: State,
    watcher: SocketWatcher,

    clients: HashMap<u64, Client>,
    next_client_id: u64,
    queue: VecDeque<QueuedCommand>,
    in_flight_execute: Option<QueuedCommand>,

    client_tasks: JoinSet<()>,
}

impl QemuSession {
    async fn new(qemu_socket: &Path) -> Result<QemuSession> {
        let stream = UnixStream::connect(qemu_socket)
            .await
            .with_context(|| format!("failed to connect to {}", qemu_socket.display()))?;

        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        let greeting = reader
            .read_json_line()
            .await
            .context("failed to read qemu greeting")?;
        let caps_request = qmp_capabilities_request_from_greeting(&greeting);
        writer
            .write_json_line(&caps_request)
            .await
            .context("failed to send qmp_capabilities to qemu")?;

        let mut capabilities = reader
            .read_json_line()
            .await
            .context("failed to read qemu capabilities reply")?;
        if let Some(capabilities) = capabilities.as_object_mut() {
            capabilities.remove("id");
        }

        let (incoming_tx, incoming_rx) = mpsc::channel(QEMU_EVENT_CHANNEL_SIZE);
        let reader_tasks = std::iter::once(async move {
            while let Ok(payload) = reader.read_json_line().await {
                if let Ok(incoming) = serde_json::from_value(payload)
                    && incoming_tx.send(incoming).await.is_err()
                {
                    break;
                }
            }
        })
        .collect();

        Ok(QemuSession {
            greeting,
            capabilities,
            write: writer,
            incoming: incoming_rx,
            _reader_tasks: reader_tasks,
        })
    }
}

impl Runtime {
    fn new(qemu_socket: PathBuf, mux_socket: PathBuf) -> Result<Self> {
        let watcher = SocketWatcher::new(&qemu_socket)?;
        Ok(Self {
            qemu_socket,
            mux_socket,
            state: State::Waiting,
            watcher,
            clients: HashMap::new(),
            next_client_id: 1,
            queue: VecDeque::with_capacity(COMMAND_QUEUE_SIZE),
            in_flight_execute: None,
            client_tasks: JoinSet::new(),
        })
    }

    async fn run(mut self) -> Result<()> {
        if self.qemu_socket_exists() {
            self.enter_connecting().await?;
        } else {
            self.enter_waiting().await?;
        }

        #[cfg(feature = "systemd")]
        sd_notify_ready().context("failed to notify readiness")?;

        loop {
            tokio::select! {
                changed = self.watcher.wait_for_change() => {
                    if let Err(err) = changed {
                        warn!(
                            "qemu_socket={} qemu_socket_watcher_error: {err:#}",
                            self.qemu_socket.display(),
                        );
                    }
                    self.handle_socket_presence_change().await?;
                }

                state_event = self.state.next_runtime_event() => {
                    match state_event {
                        StateRuntimeEvent::ConnectResult(connect_result) => {
                            self.handle_connect_result(connect_result).await?;
                        }
                        StateRuntimeEvent::ConnectRetryDue => {
                            self.start_connect_if_due();
                        }
                        StateRuntimeEvent::Accept(accept_result) => {
                            self.handle_accept_result(accept_result).await;
                        }
                        StateRuntimeEvent::Incoming(Some(incoming)) => {
                            self.handle_qemu_incoming(incoming).await?;
                        }
                        StateRuntimeEvent::Incoming(None) => {
                            self.handle_qemu_disconnect().await?;
                        }
                        StateRuntimeEvent::ClientEvent(Some(event)) => {
                            self.handle_client_event(event).await?;
                        }
                        StateRuntimeEvent::ClientEvent(None) => {
                            bail!("client event channel closed unexpectedly");
                        }
                    }
                }

                joined = async {
                    if self.client_tasks.is_empty() {
                        std::future::pending::<Option<Result<(), tokio::task::JoinError>>>().await
                    } else {
                        self.client_tasks.join_next().await
                    }
                } => {
                    if let Some(Err(err)) = joined {
                        warn!(
                            "qemu_socket={} client_task_join_error: {err}",
                            self.qemu_socket.display(),
                        );
                    }
                }
            }
        }
    }

    fn qemu_socket_exists(&self) -> bool {
        self.qemu_socket.try_exists().unwrap_or(false)
    }

    async fn enter_waiting(&mut self) -> Result<()> {
        if !self.state.is_waiting() {
            debug!(
                "qemu_socket={} transition {} -> Waiting",
                self.qemu_socket.display(),
                self.state.name()
            );
        }

        self.state = State::Waiting;
        self.remove_existing_mux_socket().await?;
        Ok(())
    }

    async fn enter_connecting(&mut self) -> Result<()> {
        if !self.state.is_connecting() {
            debug!(
                "qemu_socket={} transition {} -> Connecting",
                self.qemu_socket.display(),
                self.state.name()
            );
        }

        let old_state = mem::replace(&mut self.state, State::Waiting);
        let listener = match old_state {
            State::Connecting { listener, .. } | State::Connected { listener, .. } => listener,
            State::Waiting => self.bind_mux_listener().await?,
        };

        let (connect_results_tx, connect_results_rx) = mpsc::unbounded_channel();
        self.state = State::Connecting {
            listener,
            connect_worker: JoinSet::new(),
            connect_retry_at: Some(Instant::now()),
            connect_results_tx,
            connect_results_rx,
        };
        self.start_connect_if_due();
        Ok(())
    }

    async fn enter_connected(&mut self, session: QemuSession) -> Result<()> {
        let listener = match mem::replace(&mut self.state, State::Waiting) {
            State::Connecting { listener, .. } | State::Connected { listener, .. } => listener,
            State::Waiting => self.bind_mux_listener().await?,
        };
        let (client_event_tx, client_event_rx) = mpsc::channel(CLIENT_EVENT_CHANNEL_SIZE);
        self.state = State::Connected {
            listener,
            qemu: session,
            client_event_tx,
            client_event_rx,
        };
        self.try_send_next_command().await?;
        Ok(())
    }

    async fn handle_socket_presence_change(&mut self) -> Result<()> {
        let exists = self.qemu_socket_exists();
        debug!(
            "qemu_socket={} qemu_socket_presence_changed exists={} state={:?}",
            self.qemu_socket.display(),
            exists,
            self.state
        );
        match &self.state {
            State::Waiting => {
                if exists {
                    debug!(
                        "qemu_socket={} qemu_socket_appeared",
                        self.qemu_socket.display(),
                    );
                    self.enter_connecting().await?;
                }
            }
            State::Connecting { .. } => {
                if exists {
                    if let State::Connecting {
                        connect_retry_at, ..
                    } = &mut self.state
                    {
                        *connect_retry_at = Some(Instant::now());
                    }
                    self.start_connect_if_due();
                } else {
                    debug!(
                        "qemu_socket={} qemu_socket_disappeared_while_connecting",
                        self.qemu_socket.display(),
                    );
                    self.enter_waiting().await?;
                }
            }
            State::Connected { .. } => {
                if !exists {
                    debug!(
                        "qemu_socket={} qemu_socket_disappeared_while_connected waiting_for_stream_disconnect",
                        self.qemu_socket.display(),
                    );
                }
            }
        }

        Ok(())
    }

    fn start_connect_if_due(&mut self) {
        let State::Connecting {
            connect_worker,
            connect_retry_at,
            connect_results_tx,
            ..
        } = &mut self.state
        else {
            return;
        };
        if !connect_worker.is_empty()
            || connect_retry_at.is_none_or(|deadline| deadline > Instant::now())
        {
            return;
        }

        let qemu_socket = self.qemu_socket.clone();
        let tx = connect_results_tx.clone();
        *connect_retry_at = None;
        debug!(
            "qemu_socket={} starting_qemu_connect_attempt timeout={}ms",
            self.qemu_socket.display(),
            CONNECT_TIMEOUT.as_millis()
        );
        connect_worker.spawn(async move {
            let result = match timeout(CONNECT_TIMEOUT, QemuSession::new(&qemu_socket)).await {
                Ok(result) => result,
                Err(_) => Err(anyhow!(
                    "connect timeout after {}ms",
                    CONNECT_TIMEOUT.as_millis()
                )),
            };
            let _ = tx.send(result);
        });
    }

    async fn handle_connect_result(&mut self, connect_result: Result<QemuSession>) -> Result<()> {
        if !self.state.is_connecting() {
            return Ok(());
        }
        if let State::Connecting { connect_worker, .. } = &mut self.state {
            let _ = connect_worker.join_next().await;
        }

        match connect_result {
            Ok(session) => {
                debug!(
                    "qemu_socket={} qemu_connect_attempt_succeeded",
                    self.qemu_socket.display(),
                );
                if !self.qemu_socket_exists() {
                    self.enter_waiting().await?;
                    return Ok(());
                }
                self.enter_connected(session).await?;
            }
            Err(err) => {
                warn!(
                    "qemu_socket={} failed_to_connect_to_qemu: {err:#}",
                    self.qemu_socket.display(),
                );
                debug!(
                    "qemu_socket={} scheduling_connect_retry_in={}ms",
                    self.qemu_socket.display(),
                    CONNECT_RETRY_DELAY.as_millis()
                );
                if let State::Connecting {
                    connect_retry_at, ..
                } = &mut self.state
                {
                    *connect_retry_at = Some(Instant::now() + CONNECT_RETRY_DELAY);
                }
            }
        }

        Ok(())
    }

    async fn handle_accept_result(
        &mut self,
        accept_result: std::io::Result<(UnixStream, tokio::net::unix::SocketAddr)>,
    ) {
        let (stream, _) = match accept_result {
            Ok(conn) => conn,
            Err(err) => {
                warn!(
                    "qemu_socket={} client_accept_failed: {err}",
                    self.qemu_socket.display(),
                );
                return;
            }
        };

        let client_id = self.next_client_id;
        self.next_client_id = self.next_client_id.wrapping_add(1);

        let client_event_tx = match &self.state {
            State::Connected {
                client_event_tx, ..
            } => client_event_tx.clone(),
            _ => return,
        };

        let (sender, task) = client_task(client_id, stream, client_event_tx);
        self.client_tasks.spawn(task);
        self.clients.insert(
            client_id,
            Client {
                sender,
                pending_oob: HashSet::default(),
                handshaken: false,
            },
        );

        if let Some(greeting) = match &self.state {
            State::Connected { qemu, .. } => Some(qemu.greeting.clone()),
            _ => None,
        } {
            debug!(
                "qemu_socket={} client_id={} sending_qmp_greeting",
                self.qemu_socket.display(),
                client_id
            );
            self.send_to_client(client_id, greeting).await;
        }

        debug!(
            "qemu_socket={} client_connected id={}",
            self.qemu_socket.display(),
            client_id,
        );
    }

    async fn handle_client_event(&mut self, event: ClientEvent) -> Result<()> {
        match event {
            ClientEvent::Disconnected { client_id } => self.handle_client_disconnected(client_id),
            ClientEvent::Message { client_id, payload } => {
                self.handle_client_message(client_id, payload).await?;
            }
        }

        Ok(())
    }

    fn handle_client_disconnected(&mut self, client_id: u64) {
        self.clients.remove(&client_id);
        self.queue.retain(|cmd| cmd.client_id != client_id);
        debug!(
            "qemu_socket={} client_disconnected id={}",
            self.qemu_socket.display(),
            client_id
        );
    }

    async fn handle_client_message(&mut self, client_id: u64, payload: Value) -> Result<()> {
        let Ok(incoming) = serde_json::from_value::<ClientIncoming>(payload) else {
            self.send_error_to_client(
                client_id,
                ClientIncoming::Other { id: None },
                "invalid qmp command shape",
            )
            .await;
            return Ok(());
        };
        let Some(client) = self.clients.get_mut(&client_id) else {
            return Ok(());
        };

        if !client.handshaken {
            if !incoming.is_qmp_capabilities() {
                self.send_error_to_client(
                    client_id,
                    incoming,
                    "qmp_capabilities handshake required",
                )
                .await;
                return Ok(());
            }

            let State::Connected { qemu, .. } = &self.state else {
                unreachable!("state checked as connected above");
            };
            let mut capabilities = qemu.capabilities.clone();
            if let Some(obj) = capabilities.as_object_mut() {
                obj.extend(Some("id".into()).zip(incoming.id().cloned()));
            }
            client.handshaken = true;
            self.send_to_client(client_id, capabilities).await;
            return Ok(());
        }

        let Some(outgoing) = incoming.into_outgoing(client_id) else {
            return Ok(());
        };

        if !outgoing.is_oob() && self.queue.len() >= COMMAND_QUEUE_SIZE {
            debug!(
                "qemu_socket={} client_id={} rejecting_command_queue_full size={} limit={}",
                self.qemu_socket.display(),
                client_id,
                self.queue.len(),
                COMMAND_QUEUE_SIZE
            );
            self.send_error_to_client(client_id, outgoing, "command queue full")
                .await;
            return Ok(());
        }

        match outgoing {
            QemuOutgoing::OobExecute { ref id, .. }
                if self
                    .clients
                    .get(&client_id)
                    .is_some_and(|client| client.pending_oob.contains(&id.id)) =>
            {
                self.send_error_to_client(client_id, outgoing, "duplicate exec-oob id")
                    .await;
            }
            outgoing @ QemuOutgoing::OobExecute { .. } => {
                self.send_oob_to_qemu(client_id, outgoing).await?;
            }
            outgoing @ QemuOutgoing::Execute { .. } => {
                self.queue.push_back(QueuedCommand {
                    client_id,
                    payload: outgoing,
                });
                self.try_send_next_command().await?;
            }
        }

        Ok(())
    }

    async fn handle_qemu_incoming(&mut self, incoming: QemuIncoming) -> Result<()> {
        let (target, payload) = incoming.into_outgoing();

        match target {
            Route::Broadcast => {
                self.broadcast_event(payload).await;
            }
            Route::Client(client_id) => {
                if let Some(client) = self.clients.get_mut(&client_id)
                    && let Some(id) = payload.id()
                    && client.pending_oob.remove(id)
                {
                    self.send_to_client(client_id, payload).await;
                }
            }
            Route::Current => {
                if let Some(in_flight) = self.in_flight_execute.take() {
                    self.send_to_client(in_flight.client_id, payload).await;
                }
                self.try_send_next_command().await?;
            }
        }

        Ok(())
    }

    async fn handle_qemu_disconnect(&mut self) -> Result<()> {
        info!(
            "qemu_socket={} qemu_disconnected",
            self.qemu_socket.display(),
        );
        self.fail_in_flight_and_queued("qemu connection lost").await;

        if self.qemu_socket_exists() {
            self.enter_connecting().await?;
        } else {
            self.enter_waiting().await?;
        }

        Ok(())
    }

    async fn try_send_next_command(&mut self) -> Result<()> {
        if !self.state.is_connected() || self.in_flight_execute.is_some() {
            return Ok(());
        }

        let qemu_write = self.state.qemu_writer()?;

        while let Some(cmd) = self.queue.pop_front() {
            if !self.clients.contains_key(&cmd.client_id) {
                continue;
            }

            qemu_write
                .write_json_line(&cmd.payload)
                .await
                .context("failed to send command to qemu")?;
            self.in_flight_execute = Some(cmd);
            break;
        }

        Ok(())
    }

    async fn send_oob_to_qemu(&mut self, client_id: u64, payload: QemuOutgoing) -> Result<()> {
        let qemu_write = self.state.qemu_writer()?;
        let client_request_id = match &payload {
            QemuOutgoing::OobExecute { id, .. } => id.id.clone(),
            QemuOutgoing::Execute { .. } => unreachable!("in-band command sent as oob"),
        };
        let payload = serde_json::to_value(payload)?;

        qemu_write
            .write_json_line(&payload)
            .await
            .context("failed to send oob command to qemu")?;
        self.clients
            .get_mut(&client_id)
            .context("Nonexistent child")?
            .pending_oob
            .insert(client_request_id);
        Ok(())
    }

    async fn broadcast_event(&mut self, payload: impl Serialize) {
        let Ok(payload) = serde_json::to_value(payload) else {
            return;
        };
        let recipients: Vec<u64> = self
            .clients
            .iter()
            .filter_map(|(id, client)| client.handshaken.then_some(*id))
            .collect();

        for client_id in recipients {
            self.send_to_client(client_id, payload.clone()).await;
        }
    }

    async fn fail_in_flight_and_queued(&mut self, desc: &str) {
        let failed: Vec<_> = self
            .in_flight_execute
            .take()
            .into_iter()
            .chain(self.queue.drain(..))
            .collect();
        for cmd in failed {
            self.send_error_to_client(cmd.client_id, cmd.payload, desc)
                .await;
        }

        let failed_oob: Vec<_> = self
            .clients
            .iter_mut()
            .flat_map(|(c, v)| v.pending_oob.drain().zip(std::iter::repeat(*c)))
            .collect();
        for (route, client) in failed_oob {
            self.send_error_to_client(client, route, desc).await;
        }
    }

    async fn send_to_client(&mut self, client_id: u64, payload: impl Serialize) {
        let Ok(payload) = serde_json::to_value(payload) else {
            return;
        };
        let Some(sender) = self
            .clients
            .get(&client_id)
            .map(|client| client.sender.clone())
        else {
            return;
        };

        if sender.send(payload).await.is_err() {
            debug!(
                "qemu_socket={} client_channel_send_failed dropping_client id={}",
                self.qemu_socket.display(),
                client_id
            );
            self.clients.remove(&client_id);
        }
    }

    async fn send_error_to_client(&mut self, client_id: u64, command: impl ReplyExt, desc: &str) {
        self.send_to_client(client_id, command.error_reply(desc))
            .await;
    }

    async fn bind_mux_listener(&self) -> Result<UnixListener> {
        self.remove_existing_mux_socket().await?;

        let mode = tokio::fs::metadata(&self.qemu_socket)
            .await
            .with_context(|| format!("failed to stat qemu socket {}", self.qemu_socket.display()))?
            .permissions()
            .mode();

        let listener =
            UnixListener::bind(&self.mux_socket).context("failed to create tokio listener")?;
        tokio::fs::set_permissions(&self.mux_socket, std::fs::Permissions::from_mode(mode))
            .await
            .with_context(|| {
                format!(
                    "failed to set mux socket permissions at {}",
                    self.mux_socket.display()
                )
            })?;

        Ok(listener)
    }

    async fn remove_existing_mux_socket(&self) -> Result<()> {
        match tokio::fs::remove_file(&self.mux_socket).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| {
                format!(
                    "failed to remove socket file at {}",
                    self.mux_socket.display()
                )
            }),
        }
    }
}

fn client_task(
    client_id: u64,
    stream: UnixStream,
    event_tx: mpsc::Sender<ClientEvent>,
) -> (
    mpsc::Sender<Value>,
    impl Future<Output = ()> + Send + 'static,
) {
    let (read_half, write_half) = stream.into_split();
    let (write_tx, mut write_rx) = mpsc::channel::<Value>(CLIENT_CHANNEL_SIZE);

    let task = async move {
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);
        let mut inbound_buf = Vec::new();

        loop {
            tokio::select! {
                outbound = write_rx.recv() => {
                    let Some(payload) = outbound else {
                        break;
                    };
                    if writer.write_json_line(&payload).await.is_err() {
                        break;
                    }
                }
                inbound = reader.read_json_message(&mut inbound_buf) => {
                    let Ok(payload) = inbound else {
                        break;
                    };
                    if event_tx.send(ClientEvent::Message { client_id, payload }).await.is_err() {
                        break;
                    }
                }
            }
        }

        let _ = event_tx.send(ClientEvent::Disconnected { client_id }).await;
    };

    (write_tx, task)
}

fn greeting_advertises_oob(greeting: &Value) -> bool {
    greeting
        .get("QMP")
        .and_then(|qmp| qmp.get("capabilities"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|cap| cap.as_str())
        .any(|cap| cap == "oob")
}

fn qmp_capabilities_request_from_greeting(greeting: &Value) -> Value {
    [("execute", Value::from("qmp_capabilities"))]
        .into_iter()
        .chain(
            greeting_advertises_oob(greeting).then(|| ("arguments", json!({ "enable": ["oob"] }))),
        )
        .collect()
}

trait ReadJsonExt {
    async fn read_json_line(&mut self) -> Result<Value>;
    async fn read_json_message(&mut self, buffer: &mut Vec<u8>) -> Result<Value>;
}

impl<R> ReadJsonExt for BufReader<R>
where
    R: AsyncRead + Unpin,
{
    async fn read_json_line(&mut self) -> Result<Value> {
        let mut buf = Vec::new();
        let n = self.read_until(b'\n', &mut buf).await?;
        ensure!(n != 0, "stream closed");
        serde_json::from_slice(&buf).context("invalid json line")
    }

    async fn read_json_message(&mut self, buffer: &mut Vec<u8>) -> Result<Value> {
        loop {
            let start = buffer
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or(buffer.len());
            if start > 0 {
                buffer.drain(..start);
            }

            if !buffer.is_empty() {
                let mut stream = serde_json::Deserializer::from_slice(buffer).into_iter::<Value>();
                if let Some(item) = stream.next() {
                    match item {
                        Ok(value) => {
                            let consumed = stream.byte_offset();
                            buffer.drain(..consumed);
                            return Ok(value);
                        }
                        Err(err) if err.is_eof() => {}
                        Err(err) => return Err(err).context("invalid json message from stream"),
                    }
                }
            }

            let mut chunk = [0_u8; 1024];
            let read = self.read(&mut chunk).await?;
            ensure!(read != 0, "stream closed");
            buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

trait WriteJsonLineExt {
    async fn write_json_line(&mut self, payload: &impl Serialize) -> Result<()>;
}

impl<W> WriteJsonLineExt for BufWriter<W>
where
    W: AsyncWrite + Unpin,
{
    async fn write_json_line(&mut self, payload: &impl Serialize) -> Result<()> {
        self.write_all(&serde_json::to_vec(payload)?).await?;
        self.write_all(b"\n").await?;
        self.flush().await?;
        Ok(())
    }
}

#[cfg(feature = "systemd")]
fn sd_notify_ready() -> Result<()> {
    sd_notify(b"READY=1")
}

#[cfg(feature = "systemd")]
fn sd_notify(payload: &[u8]) -> Result<()> {
    let Some(notify_socket) = std::env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };

    let notify_addr = if notify_socket.as_encoded_bytes().first() == Some(&b'@') {
        let mut abstract_name = vec![0];
        abstract_name.extend_from_slice(&notify_socket.as_encoded_bytes()[1..]);
        SocketAddr::from_abstract_name(abstract_name)
            .context("failed to build abstract NOTIFY_SOCKET address")?
    } else {
        SocketAddr::from_pathname(Path::new(&notify_socket))
            .context("failed to build pathname NOTIFY_SOCKET address")?
    };

    let sock = UnixDatagram::unbound().context("failed to create notify socket")?;
    let written = sock
        .send_to_addr(payload, &notify_addr)
        .context("failed to send NOTIFY_SOCKET payload")?;
    ensure!(
        written == payload.len(),
        "short write while notifying systemd"
    );
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args();
    let bin = args
        .next()
        .unwrap_or_else(|| String::from("ghaf-qemu-mplex"));
    let qemu_socket = args
        .next()
        .ok_or_else(|| anyhow!("usage: {bin} <qemu-qmp-socket> <mux-socket>"))?;
    let mux_socket = args
        .next()
        .ok_or_else(|| anyhow!("usage: {bin} <qemu-qmp-socket> <mux-socket>"))?;
    ensure!(
        args.next().is_none(),
        "usage: {bin} <qemu-qmp-socket> <mux-socket>"
    );

    run(qemu_socket, mux_socket).await
}

async fn run(qemu_socket: String, mux_socket: String) -> Result<()> {
    let runtime = Runtime::new(qemu_socket.into(), mux_socket.into())?;
    runtime.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::{
        net::unix::{OwnedReadHalf, OwnedWriteHalf},
        sync::oneshot,
        time::{Duration, sleep, timeout},
    };

    const STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
    const OOB_ROUTE_CONNECTION_KEY: &str = "connection";
    const OOB_ROUTE_ID_KEY: &str = "id";

    struct TestClient {
        reader: BufReader<OwnedReadHalf>,
        writer: BufWriter<OwnedWriteHalf>,
    }

    impl TestClient {
        async fn connect_raw(path: &str) -> Result<Self> {
            let stream = timeout(STARTUP_TIMEOUT, UnixStream::connect(path))
                .await
                .context("timeout connecting test client")??;
            let (read_half, write_half) = stream.into_split();
            Ok(Self {
                reader: BufReader::new(read_half),
                writer: BufWriter::new(write_half),
            })
        }

        async fn send(&mut self, payload: Value) -> Result<()> {
            self.writer.write_json_line(&payload).await
        }

        async fn send_without_newline(&mut self, payload: Value) -> Result<()> {
            self.writer
                .write_all(&serde_json::to_vec(&payload)?)
                .await?;
            self.writer.flush().await?;
            Ok(())
        }

        async fn recv(&mut self, timeout_dur: Duration) -> Result<Option<Value>> {
            match timeout(timeout_dur, self.reader.read_json_line()).await {
                Ok(Ok(value)) => Ok(Some(value)),
                Ok(Err(err)) => Err(err),
                Err(_) => Ok(None),
            }
        }

        async fn handshake(&mut self) -> Result<()> {
            let greeting = self
                .recv(Duration::from_secs(2))
                .await?
                .context("did not receive qmp greeting")?;
            ensure!(
                greeting.get("QMP").is_some(),
                "unexpected greeting: {greeting:?}"
            );

            self.send(json!({ "execute": "qmp_capabilities" })).await?;
            let caps = self
                .recv(Duration::from_secs(2))
                .await?
                .context("did not receive qmp capabilities reply")?;
            ensure!(
                caps.get("return").is_some(),
                "unexpected capabilities: {caps:?}"
            );
            Ok(())
        }
    }

    async fn wait_for_socket(path: &str) -> Result<()> {
        timeout(STARTUP_TIMEOUT, async {
            loop {
                if Path::new(path).try_exists().unwrap_or(false) {
                    break;
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("timed out waiting for socket")?;
        Ok(())
    }

    async fn wait_for_socket_missing(path: &str) -> Result<()> {
        timeout(STARTUP_TIMEOUT, async {
            loop {
                if !Path::new(path).try_exists().unwrap_or(true) {
                    break;
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("timed out waiting for socket removal")?;
        Ok(())
    }

    async fn run_stub_qemu_with_delayed_handshake(
        socket_path: PathBuf,
        delay: Duration,
        ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("failed to bind stub qemu at {}", socket_path.display()))?;
        let _ = ready_tx.send(());

        let (stream, _) = listener.accept().await.context("stub accept failed")?;
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        sleep(delay).await;
        let greeting = json!({
            "QMP": {
                "version": {
                    "qemu": { "major": 9, "minor": 2, "micro": 0 },
                    "package": "test"
                },
                "capabilities": []
            }
        });
        writer.write_json_line(&greeting).await?;

        let _caps = reader.read_json_line().await?;
        writer.write_json_line(&json!({ "return": {} })).await?;

        while let Ok(cmd) = reader.read_json_line().await {
            if let Some(id) = cmd.get("id") {
                writer
                    .write_json_line(&json!({ "return": {"ok": true}, "id": id }))
                    .await?;
            } else {
                writer
                    .write_json_line(&json!({ "return": {"ok": true} }))
                    .await?;
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn waiting_removes_mux_socket() -> Result<()> {
        let tmp = TempDir::new()?;
        let qemu_socket = tmp.path().join("qemu.sock");
        let mux_socket = tmp.path().join("mux.sock");

        let qemu = qemu_socket.to_string_lossy().into_owned();
        let mux = mux_socket.to_string_lossy().into_owned();

        let mux_task = tokio::spawn(async move { run(qemu, mux).await });

        sleep(Duration::from_millis(200)).await;
        ensure!(
            !mux_socket.try_exists().unwrap_or(false),
            "mux socket should not exist in Waiting state"
        );

        mux_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn connecting_holds_new_connections_until_connected() -> Result<()> {
        let tmp = TempDir::new()?;
        let qemu_socket = tmp.path().join("qemu.sock");
        let mux_socket = tmp.path().join("mux.sock");

        let (ready_tx, ready_rx) = oneshot::channel();
        let stub_task = tokio::spawn(run_stub_qemu_with_delayed_handshake(
            qemu_socket.clone(),
            Duration::from_millis(600),
            ready_tx,
        ));
        ready_rx.await.context("stub qemu not ready")?;

        let qemu = qemu_socket.to_string_lossy().into_owned();
        let mux = mux_socket.to_string_lossy().into_owned();
        let mux_task = tokio::spawn(async move { run(qemu, mux).await });

        let mux_socket_str = mux_socket.to_string_lossy().into_owned();
        wait_for_socket(&mux_socket_str).await?;

        let mut client = TestClient::connect_raw(&mux_socket_str).await?;
        let early = client.recv(Duration::from_millis(150)).await?;
        ensure!(early.is_none(), "greeting arrived before Connected state");

        let greeting = client
            .recv(Duration::from_secs(2))
            .await?
            .context("did not receive greeting after connection")?;
        ensure!(
            greeting.get("QMP").is_some(),
            "unexpected greeting: {greeting:?}"
        );

        client
            .send(json!({ "execute": "qmp_capabilities" }))
            .await?;
        let caps = client
            .recv(Duration::from_secs(2))
            .await?
            .context("did not receive capabilities reply")?;
        ensure!(caps.get("return").is_some(), "unexpected caps: {caps:?}");
        ensure!(
            caps.get("id").is_none(),
            "capabilities reply should not include id when request has no id: {caps:?}"
        );

        client
            .send_without_newline(json!({ "execute": "qmp_capabilities", "id": "caps-1" }))
            .await?;
        let caps_with_id = client
            .recv(Duration::from_secs(2))
            .await?
            .context("did not receive capabilities reply with id")?;
        ensure!(
            caps_with_id.get("return").is_some(),
            "unexpected caps with id: {caps_with_id:?}"
        );
        ensure!(
            caps_with_id.get("id") == Some(&json!("caps-1")),
            "capabilities reply id mismatch: {caps_with_id:?}"
        );

        mux_task.abort();
        stub_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn mux_socket_removed_when_qemu_socket_disappears() -> Result<()> {
        let tmp = TempDir::new()?;
        let qemu_socket = tmp.path().join("qemu.sock");
        let mux_socket = tmp.path().join("mux.sock");

        let (ready_tx, ready_rx) = oneshot::channel();
        let stub_task = tokio::spawn(run_stub_qemu_with_delayed_handshake(
            qemu_socket.clone(),
            Duration::from_millis(10),
            ready_tx,
        ));
        ready_rx.await.context("stub qemu not ready")?;

        let qemu = qemu_socket.to_string_lossy().into_owned();
        let mux = mux_socket.to_string_lossy().into_owned();
        let mux_task = tokio::spawn(async move { run(qemu, mux).await });

        let mux_socket_str = mux_socket.to_string_lossy().into_owned();
        wait_for_socket(&mux_socket_str).await?;

        std::fs::remove_file(&qemu_socket)
            .with_context(|| format!("failed to remove {}", qemu_socket.display()))?;
        stub_task.abort();

        wait_for_socket_missing(&mux_socket_str).await?;
        mux_task.abort();
        Ok(())
    }

    #[test]
    fn qmp_capabilities_request_enables_oob_only_when_advertised() {
        let no_oob = json!({
            "QMP": {
                "capabilities": []
            }
        });
        assert_eq!(
            qmp_capabilities_request_from_greeting(&no_oob),
            json!({"execute": "qmp_capabilities"})
        );

        let with_oob = json!({
            "QMP": {
                "capabilities": ["oob"]
            }
        });
        assert_eq!(
            qmp_capabilities_request_from_greeting(&with_oob),
            json!({
                "execute": "qmp_capabilities",
                "arguments": {
                    "enable": ["oob"]
                }
            })
        );
    }

    #[tokio::test]
    async fn read_json_message_parses_multiple_messages_from_single_buffer() -> Result<()> {
        let raw = br#"{"a":1}{"b":2}"#;
        let cursor = std::io::Cursor::new(raw.to_vec());
        let mut reader = BufReader::new(cursor);
        let mut buffer = Vec::new();

        let first = reader.read_json_message(&mut buffer).await?;
        assert_eq!(first, json!({"a": 1}));

        let second = reader.read_json_message(&mut buffer).await?;
        assert_eq!(second, json!({"b": 2}));

        Ok(())
    }

    #[tokio::test]
    async fn read_json_message_skips_leading_whitespace() -> Result<()> {
        let raw = b" \n\t  {\"k\":\"v\"}";
        let cursor = std::io::Cursor::new(raw.to_vec());
        let mut reader = BufReader::new(cursor);
        let mut buffer = Vec::new();

        let parsed = reader.read_json_message(&mut buffer).await?;
        assert_eq!(parsed, json!({"k": "v"}));

        Ok(())
    }

    #[tokio::test]
    async fn exec_oob_routes_by_wrapped_id_while_execute_keeps_old_inflight_behavior() -> Result<()>
    {
        let tmp = TempDir::new()?;
        let qemu_socket = tmp.path().join("qemu.sock");
        let mux_socket = tmp.path().join("mux.sock");
        let qemu_socket_for_stub = qemu_socket.clone();

        let (ready_tx, ready_rx) = oneshot::channel();
        let stub_task = tokio::spawn(async move {
            let listener = UnixListener::bind(&qemu_socket_for_stub).with_context(|| {
                format!(
                    "failed to bind stub qemu at {}",
                    qemu_socket_for_stub.display()
                )
            })?;
            let _ = ready_tx.send(());

            let (stream, _) = listener.accept().await.context("stub accept failed")?;
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut writer = BufWriter::new(write_half);

            let greeting = json!({
                "QMP": {
                    "version": {
                        "qemu": { "major": 9, "minor": 2, "micro": 0 },
                        "package": "test"
                    },
                    "capabilities": ["oob"]
                }
            });
            writer.write_json_line(&greeting).await?;

            let caps_req = reader.read_json_line().await?;
            ensure!(
                caps_req
                    == json!({
                        "execute": "qmp_capabilities",
                        "arguments": { "enable": ["oob"] }
                    }),
                "unexpected capabilities request: {caps_req:?}"
            );
            writer.write_json_line(&json!({"return": {}})).await?;

            let cmd_a = reader.read_json_line().await?;
            let cmd_b = reader.read_json_line().await?;

            let (regular_cmd, oob_cmd) = if cmd_a.get("exec-oob").is_some() {
                (cmd_b, cmd_a)
            } else {
                (cmd_a, cmd_b)
            };

            ensure!(
                regular_cmd.get("execute") == Some(&json!("query-status")),
                "unexpected regular command: {regular_cmd:?}"
            );
            ensure!(
                regular_cmd.get("id") == Some(&json!({"id": "regular-1"})),
                "regular command id must be preserved: {regular_cmd:?}"
            );
            ensure!(
                oob_cmd.get("exec-oob") == Some(&json!("query-status")),
                "unexpected oob command: {oob_cmd:?}"
            );

            let oob_id = oob_cmd
                .get("id")
                .and_then(Value::as_object)
                .context("oob command id is not an object")?;
            ensure!(
                oob_id
                    .get(OOB_ROUTE_CONNECTION_KEY)
                    .and_then(Value::as_u64)
                    .is_some(),
                "oob id missing connection: {oob_cmd:?}"
            );
            ensure!(
                oob_id.get(OOB_ROUTE_ID_KEY) == Some(&json!("oob-1")),
                "oob id missing original id: {oob_cmd:?}"
            );

            writer
                .write_json_line(&json!({
                    "return": { "kind": "oob" },
                    "id": oob_cmd.get("id").cloned().context("missing oob id")?
                }))
                .await?;
            sleep(Duration::from_millis(150)).await;
            writer
                .write_json_line(&json!({
                    "return": { "kind": "regular" },
                    "id": "regular-1"
                }))
                .await?;

            Ok::<(), anyhow::Error>(())
        });
        ready_rx.await.context("stub qemu not ready")?;

        let qemu = qemu_socket.to_string_lossy().into_owned();
        let mux = mux_socket.to_string_lossy().into_owned();
        let mux_task = tokio::spawn(async move { run(qemu, mux).await });

        let mux_socket_str = mux_socket.to_string_lossy().into_owned();
        wait_for_socket(&mux_socket_str).await?;

        let mut client1 = TestClient::connect_raw(&mux_socket_str).await?;
        let mut client2 = TestClient::connect_raw(&mux_socket_str).await?;
        client1.handshake().await?;
        client2.handshake().await?;

        client1
            .send(json!({"execute": "query-status", "id": "regular-1"}))
            .await?;
        client2
            .send(json!({"exec-oob": "query-status", "id": "oob-1"}))
            .await?;

        let oob_reply = client2
            .recv(Duration::from_secs(2))
            .await?
            .context("did not receive oob reply")?;
        ensure!(
            oob_reply.get("id") == Some(&json!("oob-1")),
            "oob reply id not restored: {oob_reply:?}"
        );
        ensure!(
            oob_reply.pointer("/return/kind") == Some(&json!("oob")),
            "unexpected oob reply payload: {oob_reply:?}"
        );

        let regular_early = client1.recv(Duration::from_millis(80)).await?;
        ensure!(
            regular_early.is_none(),
            "regular reply should not arrive before stub sends it: {regular_early:?}"
        );

        let regular_reply = client1
            .recv(Duration::from_secs(2))
            .await?
            .context("did not receive regular execute reply")?;
        ensure!(
            regular_reply.get("id") == Some(&json!("regular-1")),
            "regular reply id mismatch: {regular_reply:?}"
        );
        ensure!(
            regular_reply.pointer("/return/kind") == Some(&json!("regular")),
            "unexpected regular reply payload: {regular_reply:?}"
        );

        mux_task.abort();
        stub_task.abort();
        Ok(())
    }
}
