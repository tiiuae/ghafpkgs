// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! `ClamAV` proxy with command filtering and rate limiting.
//!
//! Accepts connections from guest VMs via vsock, filters `ClamAV` commands to
//! allow only safe operations (INSTREAM, PING, VERSION), and forwards to clamd.
//!
//! This prevents guests from executing dangerous commands like SCAN (which
//! could access host files) or SHUTDOWN (which would kill the scanner).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::{Buf, BytesMut};
use clap::Parser;
use log::{debug, error, info, warn};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_vsock::{VsockAddr, VsockListener};

use ghaf_virtiofs_tools::util::{init_logger, wait_for_shutdown};

// =============================================================================
// CLI
// =============================================================================

/// Vsock CID for host (guest-to-host communication)
const VMADDR_CID_HOST: u32 = 2;

/// Default vsock port for proxy
const DEFAULT_VSOCK_PORT: u32 = 3400;

/// Default path to `ClamAV` daemon socket
const DEFAULT_CLAMD_SOCKET: &str = "/run/clamav/clamd.ctl";

/// Default maximum concurrent connections (should not exceed clamd's `MaxThreads`)
const DEFAULT_MAX_CONNECTIONS: usize = 10;

/// Default maximum total INSTREAM size (matches clamd `StreamMaxLength`)
const DEFAULT_MAX_STREAM_SIZE: u64 = 100 * 1024 * 1024;

/// Default maximum single chunk size
const DEFAULT_MAX_CHUNK_SIZE: u32 = 25 * 1024 * 1024;

/// Default command read timeout in seconds (matches clamd `CommandReadTimeout`)
const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 30;

/// Default per-read timeout in seconds (below clamd's 120s `ReadTimeout`)
const DEFAULT_READ_TIMEOUT_SECS: u64 = 60;

/// Default total INSTREAM operation timeout in seconds
const DEFAULT_STREAM_TIMEOUT_SECS: u64 = 120;

/// Buffer size for simple command responses (PING, VERSION)
const RESPONSE_BUFFER_SIZE: usize = 1024;

/// Buffer size for reading INSTREAM chunks
const CHUNK_READ_BUFFER_SIZE: usize = 8192;

#[derive(Parser)]
#[command(name = "clamd-vproxy")]
#[command(about = "ClamAV vsock proxy with command filtering")]
struct Cli {
    /// Vsock CID to bind to (2=host, 4294967295=any)
    #[arg(short, long, default_value_t = VMADDR_CID_HOST)]
    cid: u32,

    /// Vsock port to listen on
    #[arg(short, long, default_value_t = DEFAULT_VSOCK_PORT)]
    port: u32,

    /// `ClamAV` daemon socket path
    #[arg(short = 'C', long, default_value = DEFAULT_CLAMD_SOCKET)]
    clamd: PathBuf,

    /// Enable debug logging
    #[arg(short, long, default_value = "false")]
    debug: bool,

    /// Maximum concurrent connections (should not exceed clamd's `MaxThreads`)
    #[arg(long, default_value_t = DEFAULT_MAX_CONNECTIONS)]
    max_connections: usize,

    /// Maximum total INSTREAM size in bytes (matches clamd `StreamMaxLength`)
    #[arg(long, default_value_t = DEFAULT_MAX_STREAM_SIZE)]
    max_stream_size: u64,

    /// Maximum single chunk size in bytes
    #[arg(long, default_value_t = DEFAULT_MAX_CHUNK_SIZE)]
    max_chunk_size: u32,

    /// Command read timeout in seconds (matches clamd `CommandReadTimeout`)
    #[arg(long, default_value_t = DEFAULT_COMMAND_TIMEOUT_SECS)]
    command_timeout_secs: u64,

    /// Per-read timeout in seconds (below clamd's 120s `ReadTimeout`)
    #[arg(long, default_value_t = DEFAULT_READ_TIMEOUT_SECS)]
    read_timeout_secs: u64,

    /// Total INSTREAM operation timeout in seconds
    #[arg(long, default_value_t = DEFAULT_STREAM_TIMEOUT_SECS)]
    stream_timeout_secs: u64,
}

/// Runtime limits for INSTREAM operations
#[derive(Clone, Copy)]
struct StreamLimits {
    max_stream_size: u64,
    max_chunk_size: u32,
    command_timeout: Duration,
    read_timeout: Duration,
    stream_timeout: Duration,
}

impl From<&Cli> for StreamLimits {
    fn from(cli: &Cli) -> Self {
        Self {
            max_stream_size: cli.max_stream_size,
            max_chunk_size: cli.max_chunk_size,
            command_timeout: Duration::from_secs(cli.command_timeout_secs),
            read_timeout: Duration::from_secs(cli.read_timeout_secs),
            stream_timeout: Duration::from_secs(cli.stream_timeout_secs),
        }
    }
}

// =============================================================================
// Command Filtering
// =============================================================================

/// Allowed `ClamAV` commands.
///
/// All other commands are blocked, including:
/// - SCAN/CONTSCAN/MULTISCAN: Could access host filesystem
/// - SHUTDOWN: Would kill clamd
/// - RELOAD: denial-of-service via forced signature reload
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Instream,
    Ping,
    Version,
}

impl Command {
    /// Parse and validate a command buffer. Returns the command type and its byte length.
    /// Only matches exact allowed commands - rejects everything else.
    fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        let delim_pos = buf.iter().position(|&b| b == b'\0' || b == b'\n')?;
        match &buf[..=delim_pos] {
            b"zINSTREAM\0" | b"nINSTREAM\n" => Some((Self::Instream, 10)),
            b"zPING\0" | b"nPING\n" => Some((Self::Ping, 6)),
            b"zVERSION\0" | b"nVERSION\n" => Some((Self::Version, 9)),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Instream => "INSTREAM",
            Self::Ping => "PING",
            Self::Version => "VERSION",
        }
    }

    const fn is_instream(self) -> bool {
        matches!(self, Self::Instream)
    }
}

// =============================================================================
// Connection Handler
// =============================================================================

/// Maximum command length (longest allowed is "zINSTREAM\0" = 10 bytes)
const MAX_COMMAND_LEN: usize = 10;

async fn handle_connection<S>(
    mut client: S,
    clamd_path: &Path,
    conn_id: usize,
    limits: StreamLimits,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut cmd_buf = [0u8; MAX_COMMAND_LEN];
    let mut cmd_len = 0;

    let command_result = timeout(limits.command_timeout, async {
        loop {
            if cmd_len >= MAX_COMMAND_LEN {
                return Err(anyhow::anyhow!("command too long"));
            }

            let n = client
                .read(&mut cmd_buf[cmd_len..])
                .await
                .context("Failed to read command")?;

            if n == 0 {
                if cmd_len == 0 {
                    return Err(anyhow::anyhow!("client disconnected"));
                }
                return Err(anyhow::anyhow!("client disconnected mid-command"));
            }

            let old_len = cmd_len;
            cmd_len += n;

            // Check if we received a delimiter
            let new_bytes = &cmd_buf[old_len..cmd_len];
            if new_bytes.contains(&b'\0') || new_bytes.contains(&b'\n') {
                return Ok(cmd_len);
            }
        }
    })
    .await;

    let cmd_len = match command_result {
        Ok(Ok(len)) => len,
        Ok(Err(e)) => {
            debug!("[{conn_id}] {e}");
            return Ok(());
        }
        Err(_) => {
            warn!("[{conn_id}] Blocked: command timeout");
            client.write_all(b"ERROR: Command timeout\n").await.ok();
            return Ok(());
        }
    };

    let buf = &cmd_buf[..cmd_len];

    // Parse and validate - must match an allowed command exactly
    let Some((command, command_len)) = Command::parse(buf) else {
        warn!("[{conn_id}] Blocked: command not allowed");
        client.write_all(b"ERROR: Command not allowed\n").await.ok();
        return Ok(());
    };

    let remainder = &buf[command_len..];

    // For non-INSTREAM commands, reject if there are extra bytes
    if !command.is_instream() && !remainder.is_empty() {
        warn!("[{conn_id}] Blocked: unexpected data after command");
        client.write_all(b"ERROR: Command not allowed\n").await.ok();
        return Ok(());
    }

    debug!("[{conn_id}] Command: {}", command.name());

    // Connect to clamd with timeout
    let mut clamd = timeout(limits.read_timeout, UnixStream::connect(clamd_path))
        .await
        .context("Timeout connecting to clamd")?
        .context("Failed to connect to clamd")?;

    // Forward only the validated command bytes to clamd
    clamd
        .write_all(&buf[..command_len])
        .await
        .context("Failed to forward command to clamd")?;

    // Handle based on command type
    if command.is_instream() {
        handle_instream(&mut client, &mut clamd, conn_id, remainder.to_vec(), limits).await?;
    } else {
        handle_simple_response(&mut client, &mut clamd, limits.read_timeout).await?;
    }

    info!("[{conn_id}] Completed: {}", command.name());
    Ok(())
}

/// Handle simple command response (PING, VERSION)
async fn handle_simple_response<S>(
    client: &mut S,
    clamd: &mut UnixStream,
    read_timeout: Duration,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Read response from clamd with timeout
    let mut response = vec![0u8; RESPONSE_BUFFER_SIZE];
    let n = timeout(read_timeout, clamd.read(&mut response))
        .await
        .context("Timeout reading clamd response")?
        .context("Failed to read clamd response")?;

    // Send back to client
    client.write_all(&response[..n]).await?;

    Ok(())
}

/// Handle INSTREAM data transfer
///
/// Protocol:
/// 1. Client sends chunks: <4-byte BE size><data>
/// 2. Client sends end marker: <4-byte 0x00000000>
/// 3. Server responds: "stream: OK\n" or "stream: <virus> FOUND\n"
async fn handle_instream<S>(
    client: &mut S,
    clamd: &mut UnixStream,
    conn_id: usize,
    initial_data: Vec<u8>,
    limits: StreamLimits,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    timeout(limits.stream_timeout, async {
        let mut total_bytes: u64 = 0;
        let mut pending = BytesMut::from(initial_data.as_slice());

        loop {
            while pending.len() < 4 {
                let mut buf = [0u8; 4];
                let n = timeout(limits.read_timeout, client.read(&mut buf))
                    .await
                    .context("Timeout reading chunk header")?
                    .context("Failed to read chunk size")?;
                if n == 0 {
                    return Err(anyhow::anyhow!("Client disconnected during chunk header"));
                }
                pending.extend_from_slice(&buf[..n]);
            }

            let chunk_size = pending.get_u32();

            if chunk_size > limits.max_chunk_size {
                warn!(
                    "[{conn_id}] Blocked: chunk size {} exceeds limit {}",
                    chunk_size, limits.max_chunk_size
                );
                return Err(anyhow::anyhow!("Chunk size exceeds limit"));
            }

            if total_bytes + u64::from(chunk_size) > limits.max_stream_size {
                warn!(
                    "[{conn_id}] Blocked: stream size would exceed limit {}",
                    limits.max_stream_size
                );
                return Err(anyhow::anyhow!("Stream size exceeds limit"));
            }

            clamd.write_all(&chunk_size.to_be_bytes()).await?;

            if chunk_size == 0 {
                debug!("[{conn_id}] INSTREAM complete, {total_bytes} bytes");
                break;
            }

            let chunk_size = chunk_size as usize;
            total_bytes += chunk_size as u64;

            let mut remaining = chunk_size;
            if !pending.is_empty() {
                let use_len = pending.len().min(remaining);
                let chunk_data = pending.split_to(use_len);
                clamd
                    .write_all(&chunk_data)
                    .await
                    .context("Failed to forward to clamd")?;
                remaining -= use_len;
            }

            let mut buf = [0u8; CHUNK_READ_BUFFER_SIZE];
            while remaining > 0 {
                let to_read = buf.len().min(remaining);
                let n = timeout(limits.read_timeout, client.read(&mut buf[..to_read]))
                    .await
                    .context("Timeout reading chunk data")?
                    .context("Failed to read chunk data")?;
                if n == 0 {
                    return Err(anyhow::anyhow!("Client disconnected during transfer"));
                }
                clamd
                    .write_all(&buf[..n])
                    .await
                    .context("Failed to forward to clamd")?;
                remaining -= n;
            }
        }

        let mut response = vec![0u8; RESPONSE_BUFFER_SIZE];
        let n = timeout(limits.read_timeout, clamd.read(&mut response))
            .await
            .context("Timeout reading clamd response")?
            .context("Failed to read clamd response")?;

        // Log scan result
        let response_str = String::from_utf8_lossy(&response[..n]);
        if response_str.contains("FOUND") {
            warn!(
                "[{conn_id}] Scan result: INFECTED - {}",
                response_str.trim()
            );
        } else {
            debug!("[{conn_id}] Scan result: {}", response_str.trim());
        }

        client.write_all(&response[..n]).await?;
        Ok(())
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "INSTREAM timeout exceeded ({}s)",
            limits.stream_timeout.as_secs()
        )
    })?
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logger(cli.debug)?;

    info!("clamd-vproxy: starting");

    // Verify clamd socket exists
    if !cli.clamd.exists() {
        return Err(anyhow::anyhow!(
            "ClamAV socket not found: {}",
            cli.clamd.display()
        ));
    }

    let addr = VsockAddr::new(cli.cid, cli.port);
    let listener = VsockListener::bind(addr)
        .with_context(|| format!("Failed to bind to vsock CID {} port {}", cli.cid, cli.port))?;

    let limits = StreamLimits::from(&cli);

    info!(
        "clamd-vproxy: ready (vsock={}:{}, clamd={}, max_conn={}, stream={}MB, chunk={}MB)",
        cli.cid,
        cli.port,
        cli.clamd.display(),
        cli.max_connections,
        limits.max_stream_size / 1024 / 1024,
        limits.max_chunk_size / 1024 / 1024,
    );

    let semaphore = Arc::new(Semaphore::new(cli.max_connections));
    let conn_counter = Arc::new(AtomicUsize::new(0));
    let clamd_path = Arc::new(cli.clamd);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, peer_addr) = accept_result.context("Accept failed")?;

                let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                    warn!("Connection rejected: max connections reached");
                    continue;
                };

                let conn_id = conn_counter.fetch_add(1, Ordering::Relaxed);
                let clamd_path = Arc::clone(&clamd_path);

                debug!("[{conn_id}] New connection from CID {}", peer_addr.cid());

                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, &clamd_path, conn_id, limits).await {
                        error!("[{conn_id}] Error: {e}");
                    }
                    drop(permit);
                });
            }
            result = wait_for_shutdown() => {
                result?;
                break;
            }
        }
    }

    info!("clamd-vproxy: stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Valid INSTREAM commands
    #[test]
    fn parse_instream_z() {
        let result = Command::parse(b"zINSTREAM\0");
        assert_eq!(result, Some((Command::Instream, 10)));
    }

    #[test]
    fn parse_instream_n() {
        let result = Command::parse(b"nINSTREAM\n");
        assert_eq!(result, Some((Command::Instream, 10)));
    }

    #[test]
    fn parse_instream_with_trailing_data() {
        // INSTREAM followed by chunk data - should still parse command
        let result = Command::parse(b"zINSTREAM\0\x00\x00\x00\x05hello");
        assert_eq!(result, Some((Command::Instream, 10)));
    }

    // Valid PING commands
    #[test]
    fn parse_ping_z() {
        let result = Command::parse(b"zPING\0");
        assert_eq!(result, Some((Command::Ping, 6)));
    }

    #[test]
    fn parse_ping_n() {
        let result = Command::parse(b"nPING\n");
        assert_eq!(result, Some((Command::Ping, 6)));
    }

    // Valid VERSION commands
    #[test]
    fn parse_version_z() {
        let result = Command::parse(b"zVERSION\0");
        assert_eq!(result, Some((Command::Version, 9)));
    }

    #[test]
    fn parse_version_n() {
        let result = Command::parse(b"nVERSION\n");
        assert_eq!(result, Some((Command::Version, 9)));
    }

    // Blocked commands
    #[test]
    fn parse_scan_blocked() {
        assert_eq!(Command::parse(b"zSCAN\0/etc/passwd"), None);
        assert_eq!(Command::parse(b"nSCAN\n/etc/passwd"), None);
    }

    #[test]
    fn parse_contscan_blocked() {
        assert_eq!(Command::parse(b"zCONTSCAN\0/home"), None);
        assert_eq!(Command::parse(b"nCONTSCAN\n/home"), None);
    }

    #[test]
    fn parse_multiscan_blocked() {
        assert_eq!(Command::parse(b"zMULTISCAN\0/"), None);
        assert_eq!(Command::parse(b"nMULTISCAN\n/"), None);
    }

    #[test]
    fn parse_shutdown_blocked() {
        assert_eq!(Command::parse(b"zSHUTDOWN\0"), None);
        assert_eq!(Command::parse(b"nSHUTDOWN\n"), None);
    }

    #[test]
    fn parse_reload_blocked() {
        assert_eq!(Command::parse(b"zRELOAD\0"), None);
        assert_eq!(Command::parse(b"nRELOAD\n"), None);
    }

    // Invalid inputs
    #[test]
    fn parse_no_delimiter() {
        assert_eq!(Command::parse(b"zPING"), None);
        assert_eq!(Command::parse(b"zINSTREAM"), None);
    }

    #[test]
    fn parse_empty() {
        assert_eq!(Command::parse(b""), None);
    }

    #[test]
    fn parse_just_delimiter() {
        assert_eq!(Command::parse(b"\0"), None);
        assert_eq!(Command::parse(b"\n"), None);
    }

    #[test]
    fn parse_wrong_prefix() {
        // Must be 'z' or 'n' prefix
        assert_eq!(Command::parse(b"xPING\0"), None);
        assert_eq!(Command::parse(b"PING\0"), None);
        assert_eq!(Command::parse(b"aPING\n"), None);
    }

    #[test]
    fn parse_wrong_delimiter() {
        // z commands need \0, n commands need \n
        assert_eq!(Command::parse(b"zPING\n"), None);
        assert_eq!(Command::parse(b"nPING\0"), None);
    }

    #[test]
    fn parse_case_sensitive() {
        assert_eq!(Command::parse(b"zping\0"), None);
        assert_eq!(Command::parse(b"zPing\0"), None);
        assert_eq!(Command::parse(b"zinstream\0"), None);
    }

    #[test]
    fn parse_extra_whitespace() {
        assert_eq!(Command::parse(b"zPING \0"), None);
        assert_eq!(Command::parse(b" zPING\0"), None);
        assert_eq!(Command::parse(b"z PING\0"), None);
    }

    // Command helpers
    #[test]
    fn command_name() {
        assert_eq!(Command::Instream.name(), "INSTREAM");
        assert_eq!(Command::Ping.name(), "PING");
        assert_eq!(Command::Version.name(), "VERSION");
    }

    #[test]
    fn command_is_instream() {
        assert!(Command::Instream.is_instream());
        assert!(!Command::Ping.is_instream());
        assert!(!Command::Version.is_instream());
    }
}
