// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info, warn};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener};

use ghaf_virtiofs_tools::util::{REFRESH_TRIGGER_FILE, init_logger, wait_for_shutdown};

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
        debug!("Channel '{}' mapped to {}", channel, path.display());
    }

    info!(
        "virtiofs-notify: starting (port={}, channels={})",
        cli.port,
        mappings.len()
    );
    run(cli.port, mappings).await
}

async fn run(port: u32, mappings: HashMap<String, PathBuf>) -> Result<()> {
    let addr = VsockAddr::new(VMADDR_CID_ANY, port);
    let listener = VsockListener::bind(addr).context("Failed to bind vsock listener")?;
    let mappings = Arc::new(mappings);

    info!("virtiofs-notify: ready");

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, peer)) => {
                        debug!("Connection from CID {}", peer.cid());
                        let mappings = Arc::clone(&mappings);
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
                break;
            }
        }
    }

    info!("virtiofs-notify: stopped");
    Ok(())
}

async fn handle_connection(
    stream: tokio_vsock::VsockStream,
    mappings: &HashMap<String, PathBuf>,
) -> Result<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let channel = line.trim();
        if channel.is_empty() {
            continue;
        }

        let Some(base_path) = mappings.get(channel) else {
            debug!("Unknown channel '{channel}', ignoring");
            continue;
        };

        trigger_refresh(base_path)?;
        info!(
            "Triggered refresh on {} for channel '{channel}'",
            base_path.display()
        );
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mapping_valid() {
        let result = parse_mapping("documents=/mnt/share/documents");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "documents");
        assert_eq!(path, PathBuf::from("/mnt/share/documents"));
    }

    #[test]
    fn parse_mapping_with_equals_in_path() {
        // Path contains '=' - should only split on first occurrence
        let result = parse_mapping("channel=/path/with=equals");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "channel");
        assert_eq!(path, PathBuf::from("/path/with=equals"));
    }

    #[test]
    fn parse_mapping_empty_channel() {
        let result = parse_mapping("=/mnt/share");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "");
        assert_eq!(path, PathBuf::from("/mnt/share"));
    }

    #[test]
    fn parse_mapping_empty_path() {
        let result = parse_mapping("channel=");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "channel");
        assert_eq!(path, PathBuf::from(""));
    }

    #[test]
    fn parse_mapping_no_equals() {
        let result = parse_mapping("no-equals-sign");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid mapping format"));
    }

    #[test]
    fn parse_mapping_empty_string() {
        let result = parse_mapping("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_mapping_just_equals() {
        let result = parse_mapping("=");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "");
        assert_eq!(path, PathBuf::from(""));
    }

    #[test]
    fn parse_mapping_spaces_preserved() {
        let result = parse_mapping("my channel=/path/with spaces/dir");
        assert!(result.is_ok());
        let (channel, path) = result.unwrap();
        assert_eq!(channel, "my channel");
        assert_eq!(path, PathBuf::from("/path/with spaces/dir"));
    }
}
