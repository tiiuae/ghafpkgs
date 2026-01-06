// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Guest daemon for on-demand virus scanning via vsock.
//!
//! Watches directories and streams modified files to the host proxy
//! via virtio-vsock using the `ClamAV` INSTREAM protocol.

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use anyhow::{Context, Result};
use clap::Parser;
use ghaf_virtiofs_tools::scanner::{ClamAVScanner, ScanResult, VirusScanner};
use ghaf_virtiofs_tools::util::{
    DEFAULT_NOTIFY_SOCKET, InfectedAction, init_logger, notify_error, notify_infected,
    wait_for_shutdown,
};
use ghaf_virtiofs_tools::watcher::{EventHandler, FileId, Watcher};
use log::{debug, error, info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_vsock::{VsockAddr, VsockStream};

/// Host CID (always 2 for guest-to-host communication)
const VMADDR_CID_HOST: u32 = 2;

/// Default vsock port for connecting to host proxy
const DEFAULT_VSOCK_PORT: u32 = 3400;

/// Buffer size for ping response
const PING_BUFFER_SIZE: usize = 64;

/// Channel capacity for pending scan requests
const SCAN_CHANNEL_CAPACITY: usize = 100;

/// Buffer size for scan response from `ClamAV`
const SCAN_RESPONSE_BUFFER_SIZE: usize = 4096;

#[derive(Parser)]
#[command(name = "clamd-vclient")]
#[command(about = "ClamAV vsock client for on-modify virus scanning")]
struct Cli {
    /// Directories to watch for file changes
    #[arg(short, long, required = true, num_args = 1..)]
    watch: Vec<PathBuf>,

    /// Directories to exclude from recursive watching
    #[arg(short, long, action = clap::ArgAction::Append)]
    exclude: Vec<PathBuf>,

    /// Vsock CID to connect to (2=host)
    #[arg(short, long, default_value_t = VMADDR_CID_HOST)]
    cid: u32,

    /// Vsock port to connect to on host (where proxy listens)
    #[arg(short, long, default_value_t = DEFAULT_VSOCK_PORT)]
    port: u32,

    /// Use local Unix socket (`ClamAV`) instead of vsock
    #[arg(short, long, default_value = "false")]
    socket: bool,

    /// Action on infected files: log, delete, quarantine
    #[arg(short, long, default_value = "delete")]
    action: InfectedAction,

    /// Quarantine directory (required if action=quarantine)
    #[arg(long)]
    quarantine_dir: Option<PathBuf>,

    /// Notification socket path (empty to disable)
    #[arg(long, default_value = DEFAULT_NOTIFY_SOCKET)]
    notify_socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long, default_value = "false")]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Validate args
    if cli.action == InfectedAction::Quarantine && cli.quarantine_dir.is_none() {
        return Err(anyhow::anyhow!(
            "--quarantine-dir is required when action=quarantine"
        ));
    }

    for dir in &cli.watch {
        if !dir.exists() {
            return Err(anyhow::anyhow!(
                "Watch directory does not exist: {}",
                dir.display()
            ));
        }
    }

    init_logger(cli.debug)?;

    // Validate connectivity
    if cli.socket {
        ClamAVScanner
            .validate_availability()
            .context("Failed to connect to local ClamAV")?;
    } else {
        let addr = VsockAddr::new(cli.cid, cli.port);
        validate_vsock_connection(addr)
            .await
            .context("Failed to connect to host proxy")?;
    }

    info!("Starting guest daemon");
    if cli.socket {
        info!("Using local ClamAV socket");
    } else {
        info!(
            "Connecting to host proxy via vsock CID {} port {}",
            cli.cid, cli.port
        );
    }
    for dir in &cli.watch {
        info!("Watching: {}", dir.display());
    }
    for dir in &cli.exclude {
        info!("Excluding: {}", dir.display());
    }
    info!("Action on infected: {:?}", cli.action);

    run(cli).await
}

/// Validate vsock connection by sending PING to proxy
async fn validate_vsock_connection(addr: VsockAddr) -> Result<()> {
    let mut stream = VsockStream::connect(addr)
        .await
        .context("Failed to connect to vsock")?;

    stream.write_all(b"zPING\0").await?;

    let mut buf = [0u8; PING_BUFFER_SIZE];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);

    if response.trim().trim_matches('\0') == "PONG" {
        info!(
            "Host proxy available via vsock CID {} port {}",
            addr.cid(),
            addr.port()
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!("Unexpected ping response: {response}"))
    }
}

async fn run(cli: Cli) -> Result<()> {
    let mut watcher = Watcher::new()?;
    watcher.set_excludes(cli.exclude.clone());

    for dir in &cli.watch {
        watcher.add_recursive(dir, "")?;
    }

    // Channel for scan requests from sync watcher to async scanner
    let (tx, mut rx) = mpsc::channel::<PathBuf>(SCAN_CHANNEL_CAPACITY);

    let mut handler = GuestHandler { tx };

    let cid = cli.cid;
    let port = cli.port;
    let action = cli.action;
    let quarantine_dir = cli.quarantine_dir.clone();
    let notify_socket = cli.notify_socket.clone();
    let use_socket = cli.socket;

    // Spawn scanner task
    let scanner_task = tokio::spawn(async move {
        while let Some(path) = rx.recv().await {
            let result = if use_socket {
                let p = path.clone();
                tokio::task::spawn_blocking(move || scan_file_socket(&p))
                    .await
                    .unwrap_or_else(|e| Err(anyhow::anyhow!("spawn_blocking failed: {e}")))
            } else {
                scan_file_vsock(&path, cid, port).await
            };

            match result {
                Ok(ref scan_result) => {
                    handle_scan_result(
                        &path,
                        scan_result,
                        action,
                        quarantine_dir.as_deref(),
                        &notify_socket,
                    );
                }
                Err(e) => {
                    error!("Scan error for {}: {e}", path.display());
                    notify_error(&notify_socket, &path, &e.to_string());
                }
            }
        }
    });

    info!("Guest daemon running. Press Ctrl+C to stop.");

    tokio::select! {
        () = watcher.run(&mut handler) => {
            info!("Watcher stream ended");
        }
        _ = wait_for_shutdown() => {}
    }

    drop(handler);
    let _ = scanner_task.await;

    info!("Guest daemon stopped");
    Ok(())
}

// =============================================================================
// Scanners
// =============================================================================

/// Scan a file using local `ClamAV` socket
fn scan_file_socket(path: &Path) -> Result<ScanResult> {
    info!("Scanning: {}", path.display());
    ClamAVScanner.scan_path(path)
}

/// Scan a file by streaming its contents to the host proxy via vsock
async fn scan_file_vsock(path: &Path, cid: u32, port: u32) -> Result<ScanResult> {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to read {}: {e}", path.display());
            return Ok(ScanResult::NotFound);
        }
    };

    if data.is_empty() {
        debug!("Skipping empty file: {}", path.display());
        return Ok(ScanResult::Clean);
    }

    info!("Scanning: {} ({} bytes)", path.display(), data.len());

    let addr = VsockAddr::new(cid, port);
    let mut stream = VsockStream::connect(addr)
        .await
        .context("Failed to connect to vsock")?;

    // Send INSTREAM command
    stream.write_all(b"zINSTREAM\0").await?;

    // Send data: size (big-endian u32) + data
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&data).await?;

    // End marker (4 zero bytes)
    stream.write_all(&[0, 0, 0, 0]).await?;

    // Read response
    let mut buf = [0u8; SCAN_RESPONSE_BUFFER_SIZE];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n])
        .trim_matches('\0')
        .trim()
        .to_string();

    Ok(ClamAVScanner::parse_response(
        &response,
        &path.display().to_string(),
    ))
}

fn handle_scan_result(
    path: &Path,
    result: &ScanResult,
    action: InfectedAction,
    quarantine_dir: Option<&Path>,
    notify_socket: &Path,
) {
    match result {
        ScanResult::Clean => {
            info!("Clean: {}", path.display());
        }
        ScanResult::Infected(ref virus_name) => {
            warn!("INFECTED: {} ({})", path.display(), virus_name);
            notify_infected(notify_socket, path, virus_name);
            handle_infected(path, action, quarantine_dir);
        }
        ScanResult::Error => {
            error!("Scan error for {}", path.display());
            notify_error(notify_socket, path, "scan failed");
        }
        ScanResult::NotFound => {
            debug!("File not found: {}", path.display());
        }
    }
}

/// Generate a unique quarantine path with timestamp to avoid collisions
fn unique_quarantine_path(qdir: &Path, source: &Path) -> PathBuf {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let ext = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    qdir.join(format!("{stem}_{timestamp}{ext}"))
}

fn handle_infected(path: &Path, action: InfectedAction, quarantine_dir: Option<&Path>) {
    match action {
        InfectedAction::Log => {
            // Already logged
        }
        InfectedAction::Delete => {
            if let Err(e) = fs::remove_file(path) {
                error!("Failed to delete infected file: {e}");
            } else {
                info!("Deleted infected file: {}", path.display());
            }
        }
        InfectedAction::Quarantine => {
            if let Some(qdir) = quarantine_dir {
                if let Err(e) = fs::create_dir_all(qdir) {
                    error!("Failed to create quarantine dir: {e}");
                    return;
                }

                let dest = unique_quarantine_path(qdir, path);

                // Try rename first, fallback to copy+delete for cross-filesystem
                let result = fs::rename(path, &dest)
                    .or_else(|_| fs::copy(path, &dest).and_then(|_| fs::remove_file(path)));

                match result {
                    Ok(()) => info!("Quarantined: {} -> {}", path.display(), dest.display()),
                    Err(e) => error!("Failed to quarantine file: {e}"),
                }
            }
        }
    }
}

// =============================================================================
// Guest Handler
// =============================================================================

struct GuestHandler {
    tx: mpsc::Sender<PathBuf>,
}

impl EventHandler for GuestHandler {
    fn on_modified(&mut self, path: &Path, _source: &str) -> Vec<(FileId, i64)> {
        if let Err(e) = self.tx.try_send(path.to_path_buf()) {
            warn!("Scan queue full, dropping: {} ({e})", path.display());
        }
        vec![] // No loop prevention needed for guest
    }

    fn on_deleted(&mut self, _path: &Path, _source: &str) {
        // Guest daemon ignores delete events
    }

    fn on_renamed(&mut self, _path: &Path, _old_path: &Path, _source: &str) -> Vec<(FileId, i64)> {
        // Guest daemon ignores rename events - same content, already scanned
        vec![]
    }
}
