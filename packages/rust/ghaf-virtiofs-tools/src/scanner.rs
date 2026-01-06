// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Virus scanning functionality for the shared directories daemon.
//!
//! This module provides a pluggable interface for virus scanning.
//! Supports both path-based and fd-based scanning for TOCTOU safety.
//!
//! Scan results distinguish between:
//! - `Clean`: File passed virus scan
//! - `Infected`: Virus/malware detected
//! - `Error`: Scan failed (permission, corrupted file, scanner unavailable)
//! - `NotFound`: File disappeared before scan
//!
//! The daemon handles `Error` based on `permissive` config (fail-safe or permissive).

use anyhow::Result;
use log::{debug, error, info, warn};
use sendfd::SendWithFd;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

const CLAMAV_SOCKET_PATH: &str = "/var/run/clamav/clamd.ctl";

/// Result of a virus scan operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanResult {
    /// File is clean
    Clean,
    /// File is infected (virus/malware detected) with signature name
    Infected(String),
    /// Scan error (permission denied, corrupted file, scanner unavailable)
    Error,
    /// File was not found (moved/deleted before scan)
    NotFound,
}

/// Input for scanning - either a path or a file descriptor.
pub enum ScanInput<'a> {
    /// Scan by file path (will open the file)
    Path(&'a Path),
    /// Scan by file descriptor (TOCTOU-safe)
    Fd(BorrowedFd<'a>),
}

impl<'a> From<&'a Path> for ScanInput<'a> {
    fn from(path: &'a Path) -> Self {
        ScanInput::Path(path)
    }
}

impl<'a, F: AsFd> From<&'a F> for ScanInput<'a> {
    fn from(fd: &'a F) -> Self {
        ScanInput::Fd(fd.as_fd())
    }
}

/// Virus scanner interface.
pub trait VirusScanner: Send + Sync {
    /// Validate that the scanner is available and functional.
    fn validate_availability(&self) -> Result<()>;

    /// Scan a file by path.
    /// Returns `ScanResult::Error` for scan failures (daemon decides handling).
    fn scan_path(&self, file_path: &Path) -> Result<ScanResult>;

    /// Scan a file by file descriptor (TOCTOU-safe).
    /// The path is used only for logging.
    /// Returns `ScanResult::Error` for scan failures (daemon decides handling).
    fn scan_fd(&self, fd: BorrowedFd<'_>, path_for_logging: &Path) -> Result<ScanResult>;

    /// Scan file content from a byte stream.
    /// Used for remote scanning over network/vsock (INSTREAM protocol).
    /// The name is used only for logging.
    fn scan_stream(&self, data: &[u8], name_for_logging: &str) -> Result<ScanResult>;

    /// Scan using either path or fd.
    fn scan(&self, input: ScanInput<'_>, path_for_logging: &Path) -> Result<ScanResult> {
        match input {
            ScanInput::Path(path) => self.scan_path(path),
            ScanInput::Fd(fd) => self.scan_fd(fd, path_for_logging),
        }
    }
}

/// `ClamAV` scanner using FILDES command via Unix socket.
/// Uses file descriptor passing for TOCTOU-safe scanning.
pub struct ClamAVScanner;

impl ClamAVScanner {
    fn ping() -> std::io::Result<String> {
        let mut stream = UnixStream::connect(CLAMAV_SOCKET_PATH)?;
        stream.write_all(b"zPING\0")?;
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n])
            .trim_matches('\0')
            .trim()
            .to_string())
    }

    fn send_fd_for_scan(fd: BorrowedFd<'_>) -> std::io::Result<String> {
        let mut stream = UnixStream::connect(CLAMAV_SOCKET_PATH)?;
        stream.write_all(b"nFILDES\n")?;
        stream.send_with_fd(&[0], &[fd.as_raw_fd()])?;
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n])
            .trim_matches('\0')
            .trim()
            .to_string())
    }

    fn send_stream_for_scan(data: &[u8]) -> std::io::Result<String> {
        let mut stream = UnixStream::connect(CLAMAV_SOCKET_PATH)?;
        stream.write_all(b"nINSTREAM\n")?;

        // Send size (big-endian u32) + data
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(data)?;

        // End marker (4 zero bytes)
        stream.write_all(&[0, 0, 0, 0])?;

        // Read response
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n])
            .trim_matches('\0')
            .trim()
            .to_string())
    }

    /// Parse a `ClamAV` response string into a `ScanResult`.
    #[must_use]
    pub fn parse_response(response: &str, name_for_logging: &str) -> ScanResult {
        if response.ends_with("OK") {
            debug!("Clean: {name_for_logging}");
            return ScanResult::Clean;
        }

        if response.ends_with("FOUND") {
            let signature = response
                .rsplit_once(": ")
                .map_or("unknown", |(_, s)| s.trim_end_matches(" FOUND"));
            warn!("Virus in {name_for_logging}: {signature}");
            return ScanResult::Infected(signature.to_string());
        }

        if response.ends_with("ERROR") {
            error!("ClamAV error for {name_for_logging}: {response}");
            return ScanResult::Error;
        }

        error!("Unexpected ClamAV response: {response}");
        ScanResult::Error
    }
}

impl VirusScanner for ClamAVScanner {
    fn validate_availability(&self) -> Result<()> {
        let response = Self::ping().map_err(|e| {
            anyhow::anyhow!("Failed to connect to ClamAV at {CLAMAV_SOCKET_PATH}: {e}")
        })?;

        if response == "PONG" {
            info!("ClamAV daemon available: {CLAMAV_SOCKET_PATH}");
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Unexpected ClamAV ping response: {response}"
            ))
        }
    }

    fn scan_path(&self, file_path: &Path) -> Result<ScanResult> {
        debug!("ClamAV scanning path: {}", file_path.display());

        let file = match File::open(file_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!("File not found: {}", file_path.display());
                return Ok(ScanResult::NotFound);
            }
            Err(e) => {
                error!("Failed to open file for scanning: {e}");
                return Ok(ScanResult::Error);
            }
        };

        self.scan_fd(file.as_fd(), file_path)
    }

    fn scan_fd(&self, fd: BorrowedFd<'_>, path_for_logging: &Path) -> Result<ScanResult> {
        debug!("ClamAV scanning fd for: {}", path_for_logging.display());

        let response = match Self::send_fd_for_scan(fd) {
            Ok(r) => r,
            Err(e) => {
                error!("ClamAV connection error: {e}");
                return Ok(ScanResult::Error);
            }
        };

        Ok(Self::parse_response(
            &response,
            &path_for_logging.display().to_string(),
        ))
    }

    fn scan_stream(&self, data: &[u8], name_for_logging: &str) -> Result<ScanResult> {
        debug!(
            "ClamAV scanning stream: {name_for_logging} ({} bytes)",
            data.len()
        );

        let response = match Self::send_stream_for_scan(data) {
            Ok(r) => r,
            Err(e) => {
                error!("ClamAV connection error: {e}");
                return Ok(ScanResult::Error);
            }
        };

        Ok(Self::parse_response(&response, name_for_logging))
    }
}
