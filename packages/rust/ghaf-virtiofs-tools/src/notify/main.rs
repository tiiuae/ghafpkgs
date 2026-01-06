// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Guest notification receiver daemon.
//!
//! Receives channel notifications from host over vsock.
//! Triggers file browser refresh by toggling a hidden temp file.
//!
//! Protocol: `channel\n`

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

use anyhow::{Context, Result};
use clap::Parser;
use ghaf_virtiofs_tools::util::{REFRESH_TRIGGER_FILE, init_logger, wait_for_shutdown};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener};

/// Default vsock port for notifications
const DEFAULT_NOTIFY_PORT: u32 = 3401;

#[derive(Parser)]
#[command(name = "virtiofs-notify")]
#[command(about = "Receive virtiofs notifications from host and trigger local refresh")]
struct Cli {
    /// Vsock port to listen on
    #[arg(short, long, default_value_t = DEFAULT_NOTIFY_PORT)]
    port: u32,

    /// Channel to path mappings: channel=path
    /// Example: -m documents=/mnt/share/documents -m media=/mnt/share/media
    #[arg(short = 'm', long = "map", value_parser = parse_mapping)]
    mappings: Vec<(String, PathBuf)>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn parse_mapping(s: &str) -> Result<(String, PathBuf), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid mapping format: {s} (expected channel=path)"
        ));
    }
    Ok((parts[0].to_string(), PathBuf::from(parts[1])))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logger(cli.debug)?;

    if cli.mappings.is_empty() {
        return Err(anyhow::anyhow!(
            "At least one channel mapping is required (-m channel=/path)"
        ));
    }

    let mappings: HashMap<String, PathBuf> = cli.mappings.into_iter().collect();

    for (channel, path) in &mappings {
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Mapped path does not exist: {} -> {}",
                channel,
                path.display()
            ));
        }
        info!("Channel '{}' mapped to {}", channel, path.display());
    }

    info!("Starting notification receiver on vsock port {}", cli.port);
    run(cli.port, mappings).await
}

async fn run(port: u32, mappings: HashMap<String, PathBuf>) -> Result<()> {
    let addr = VsockAddr::new(VMADDR_CID_ANY, port);
    let listener = VsockListener::bind(addr).context("Failed to bind vsock listener")?;

    info!("Listening for notifications on vsock port {port}");

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer)) => {
                        debug!("Connection from CID {}", peer.cid());
                        let mappings = mappings.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, &mappings).await {
                                warn!("Connection error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {e}");
                    }
                }
            }
            _ = wait_for_shutdown() => {
                info!("Shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    stream: tokio_vsock::VsockStream,
    mappings: &HashMap<String, PathBuf>,
) -> Result<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            continue;
        }
        if let Err(e) = process_notification(&line, mappings) {
            warn!("Failed to process notification '{line}': {e}");
        }
    }

    Ok(())
}

fn process_notification(line: &str, mappings: &HashMap<String, PathBuf>) -> Result<()> {
    let channel = line.trim();
    if channel.is_empty() {
        return Ok(());
    }

    let Some(base_path) = mappings.get(channel) else {
        debug!("Unknown channel '{channel}', ignoring");
        return Ok(());
    };

    trigger_refresh(base_path)?;
    info!(
        "Triggered refresh on {} for channel '{channel}'",
        base_path.display()
    );

    Ok(())
}

/// Trigger inotify CREATE or DELETE event by toggling a hidden file's existence.
/// This causes file browsers (which debounce events) to refresh their directory listing.
fn trigger_refresh(dir: &Path) -> Result<()> {
    if !dir.exists() {
        debug!(
            "Directory does not exist, skipping refresh: {}",
            dir.display()
        );
        return Ok(());
    }

    let tmp_file = dir.join(REFRESH_TRIGGER_FILE);
    if tmp_file.exists() {
        std::fs::remove_file(&tmp_file).context("Failed to remove refresh file")?;
    } else {
        std::fs::File::create(&tmp_file).context("Failed to create refresh file")?;
    }

    Ok(())
}
