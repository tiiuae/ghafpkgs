// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::Result;
use log::{debug, error, info, warn};
use sendfd::SendWithFd;

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

/// Virus scanner interface.
pub trait VirusScanner: Send + Sync {
    /// Validate that the scanner is available and functional.
    ///
    /// # Errors
    /// Returns an error if the scanner daemon is unavailable or returns an unexpected response.
    fn validate_availability(&self) -> Result<()>;

    /// Scan a file by path.
    /// Returns `ScanResult::Error` for scan failures (caller decides handling).
    ///
    /// # Errors
    /// Returns an error if communication with the scanner daemon fails.
    fn scan_path(&self, file_path: &Path) -> Result<ScanResult>;

    /// Scan a file by file descriptor (TOCTOU-safe).
    /// The path is used only for logging.
    /// Returns `ScanResult::Error` for scan failures (caller decides handling).
    ///
    /// # Errors
    /// Returns an error if communication with the scanner daemon fails.
    fn scan_fd(&self, fd: BorrowedFd<'_>, path_for_logging: &Path) -> Result<ScanResult>;
}

/// `ClamAV` scanner using FILDES command via Unix socket.
/// Uses file descriptor passing for TOCTOU-safe scanning.
pub struct ClamAVScanner;

impl ClamAVScanner {
    fn ping() -> std::io::Result<String> {
        let mut stream = UnixStream::connect(CLAMAV_SOCKET_PATH)?;
        stream.write_all(b"nPING\n")?;
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf)?;
        Ok(String::from_utf8_lossy(buf[..n].trim_ascii()).into_owned())
    }

    fn send_fd_for_scan(fd: BorrowedFd<'_>) -> std::io::Result<String> {
        let mut stream = UnixStream::connect(CLAMAV_SOCKET_PATH)?;
        stream.write_all(b"nFILDES\n")?;
        stream.send_with_fd(&[0], &[fd.as_raw_fd()])?;
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        Ok(String::from_utf8_lossy(buf[..n].trim_ascii()).into_owned())
    }

    /// Parse a `ClamAV` response string into a `ScanResult`.
    #[must_use]
    pub fn parse_response(response: &str, name_for_logging: &str) -> ScanResult {
        if response.ends_with("OK") {
            debug!("Clean: {name_for_logging}");
            return ScanResult::Clean;
        }

        if let Some(stripped) = response.strip_suffix(" FOUND") {
            let signature = stripped.rsplit_once(": ").map_or("unknown", |(_, s)| s);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response_clean() {
        let result = ClamAVScanner::parse_response("fd[0]: OK", "test.txt");
        assert_eq!(result, ScanResult::Clean);
    }

    #[test]
    fn parse_response_clean_stream() {
        let result = ClamAVScanner::parse_response("stream: OK", "stream");
        assert_eq!(result, ScanResult::Clean);
    }

    #[test]
    fn parse_response_infected_simple() {
        let result = ClamAVScanner::parse_response("fd[0]: Eicar-Test-Signature FOUND", "test.txt");
        assert_eq!(
            result,
            ScanResult::Infected("Eicar-Test-Signature".to_string())
        );
    }

    #[test]
    fn parse_response_infected_complex_signature() {
        let result =
            ClamAVScanner::parse_response("stream: Win.Trojan.Agent-123456 FOUND", "malware.exe");
        assert_eq!(
            result,
            ScanResult::Infected("Win.Trojan.Agent-123456".to_string())
        );
    }

    #[test]
    fn parse_response_infected_no_colon() {
        // Malformed response without colon - falls back to "unknown"
        let result = ClamAVScanner::parse_response("SomeVirus FOUND", "test.txt");
        assert_eq!(result, ScanResult::Infected("unknown".to_string()));
    }

    #[test]
    fn parse_response_error() {
        let result = ClamAVScanner::parse_response("fd[0]: Access denied. ERROR", "test.txt");
        assert_eq!(result, ScanResult::Error);
    }

    #[test]
    fn parse_response_error_lstat() {
        let result = ClamAVScanner::parse_response(
            "fd[0]: lstat() failed: No such file or directory. ERROR",
            "test.txt",
        );
        assert_eq!(result, ScanResult::Error);
    }

    #[test]
    fn parse_response_unexpected() {
        let result = ClamAVScanner::parse_response("UNKNOWN RESPONSE", "test.txt");
        assert_eq!(result, ScanResult::Error);
    }

    #[test]
    fn parse_response_empty() {
        let result = ClamAVScanner::parse_response("", "test.txt");
        assert_eq!(result, ScanResult::Error);
    }

    #[test]
    fn parse_response_partial_ok() {
        // "OK" must be at the end
        let result = ClamAVScanner::parse_response("OK but not really", "test.txt");
        assert_eq!(result, ScanResult::Error);
    }

    #[test]
    fn parse_response_partial_found() {
        // "FOUND" must be at the end
        let result = ClamAVScanner::parse_response("FOUND something else", "test.txt");
        assert_eq!(result, ScanResult::Error);
    }
}
