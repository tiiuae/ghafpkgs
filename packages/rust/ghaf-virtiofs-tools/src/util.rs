// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::Result;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

// =============================================================================
// Logger
// =============================================================================

/// Initialize the systemd journal logger.
///
/// # Errors
/// Returns an error if the journal logger fails to initialize.
pub fn init_logger(debug: bool) -> Result<()> {
    let log_level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    systemd_journal_logger::JournalLog::new()?.install()?;
    log::set_max_level(log_level);
    Ok(())
}

// =============================================================================
// Signal Handling
// =============================================================================

/// Shutdown signal received.
#[derive(Debug, Clone, Copy)]
pub enum ShutdownSignal {
    Sigint,
    Sigterm,
}

/// Wait for a shutdown signal (SIGINT or SIGTERM).
///
/// # Errors
/// Returns an error if signal handlers fail to initialize.
pub async fn wait_for_shutdown() -> Result<ShutdownSignal> {
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => {
            info!("SIGINT received");
            Ok(ShutdownSignal::Sigint)
        }
        _ = sigterm.recv() => {
            info!("SIGTERM received");
            Ok(ShutdownSignal::Sigterm)
        }
    }
}

// =============================================================================
// InfectedAction
// =============================================================================

/// Action to take when an infected file is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InfectedAction {
    /// Log the infection but take no action on the file.
    Log,
    /// Delete the infected file.
    #[default]
    Delete,
    /// Move the infected file to a quarantine directory.
    Quarantine,
}

impl std::fmt::Display for InfectedAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Log => write!(f, "log"),
            Self::Delete => write!(f, "delete"),
            Self::Quarantine => write!(f, "quarantine"),
        }
    }
}

impl std::str::FromStr for InfectedAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "log" => Ok(Self::Log),
            "delete" => Ok(Self::Delete),
            "quarantine" => Ok(Self::Quarantine),
            _ => Err(format!("Invalid action: {s}. Use: log, delete, quarantine")),
        }
    }
}

// =============================================================================
// Notification
// =============================================================================

/// Default notification socket path.
pub const DEFAULT_NOTIFY_SOCKET: &str = "/run/clamav/notify.sock";

/// Hidden file used to trigger file browser refresh in guests.
pub const REFRESH_TRIGGER_FILE: &str = ".virtiofs-refresh";

/// Send a notification message to the notification socket.
///
/// Skips silently if the socket doesn't exist.
/// Logs a warning if connection/write fails for an existing socket.
fn send_notification(socket_path: &Path, message: &str) {
    if !socket_path.exists() {
        return;
    }

    match UnixStream::connect(socket_path) {
        Ok(mut stream) => {
            if let Err(e) = writeln!(stream, "{message}") {
                warn!("Failed to write to notification socket: {e}");
            } else {
                debug!("Notification sent: {message}");
            }
        }
        Err(e) => {
            warn!("Notification socket unavailable: {e}");
        }
    }
}

/// Send an infected file notification.
///
/// Message format: `Malware <virus_name> was detected in file: <file_path>`
pub fn notify_infected(socket_path: &Path, file_path: &Path, virus_name: &str) {
    let message = format!(
        "Malware {} was detected in file: {}",
        virus_name,
        file_path.display()
    );
    send_notification(socket_path, &message);
}

/// Send a scan error notification.
///
/// Message format: `Scan error for file <file_path>: <error>`
pub fn notify_error(socket_path: &Path, file_path: &Path, error: &str) {
    let message = format!("Scan error for file {}: {}", file_path.display(), error);
    send_notification(socket_path, &message);
}
