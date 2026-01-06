// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Inotify-based daemon for shared directories.
//!
//! Host directory structure per channel:
//! ```text
//! ${channel}/
//! ├── share/
//! │   ├── producer1/        # virtiofs rw mount for producer1
//! │   └── producer2/        # virtiofs rw mount for producer2
//! ├── export/               # all scanned files (flat namespace)
//! ├── export-ro/            # ro bind mount of export/ for consumers
//! └── quarantine/           # infected files (if enabled)
//! ```
//!
//! Flow:
//! 1. Producer writes to share/producer1/file.txt
//! 2. `IN_CLOSE_WRITE` detected, file queued for scan with debounce
//! 3. After debounce, file scanned
//! 4. If clean: reflink to share/producer2/, share/producer3/, ..., export/
//! 5. If infected: quarantine or delete
//!
//! Delete propagation:
//! - `IN_DELETE` in share/producer1/ triggers removal from other producers + export
//!
//! Loop prevention:
//! - Track inodes we've written to avoid re-processing our own reflinks

use super::config::{ChannelConfig, Config};
use super::notify::{Notifier, build_notifier};
use anyhow::{Context, Result};
use ghaf_virtiofs_tools::scanner::{ScanResult, VirusScanner};
use ghaf_virtiofs_tools::util::{InfectedAction, notify_error, notify_infected, wait_for_shutdown};
use ghaf_virtiofs_tools::watcher::{EventHandler, FileId, Watcher, WatcherConfig};
use log::{debug, error, info, warn};
use rustix::fs::{Mode, OFlags};
use std::fs::{self, File};
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::time::sleep;

// =============================================================================
// Constants
// =============================================================================

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_CHANNEL_CAPACITY: usize = 1;

// =============================================================================
// Errors
// =============================================================================

#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("Channel '{channel}' error: {message}")]
    Channel {
        channel: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("I/O error")]
    Io(#[from] std::io::Error),
}

impl DaemonError {
    pub fn channel(
        channel: &str,
        message: &str,
        source: Option<impl std::error::Error + Send + Sync + 'static>,
    ) -> Self {
        Self::Channel {
            channel: channel.to_string(),
            message: message.to_string(),
            source: source.map(|s| Box::new(s) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
}

// =============================================================================
// Daemon
// =============================================================================

pub struct Daemon {
    config: Config,
    scanner: Arc<dyn VirusScanner + Send + Sync>,
}

impl Daemon {
    pub fn new(config: Config, scanner: Arc<dyn VirusScanner + Send + Sync>) -> Self {
        Self { config, scanner }
    }

    pub async fn run(self) -> Result<()> {
        info!(
            "Starting shared directories daemon with {} channels",
            self.config.len()
        );

        // Build notifier from config (shared across all channels)
        let notifier = Arc::new(build_notifier(&self.config));

        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(SHUTDOWN_CHANNEL_CAPACITY);
        let mut handles = Vec::new();

        for (channel_name, channel_config) in self.config {
            if let Err(errors) = channel_config.validate() {
                for err in &errors {
                    error!("Channel '{channel_name}': {err}");
                }
                continue;
            }

            let mut shutdown_rx = shutdown_tx.subscribe();
            let scanner = Arc::clone(&self.scanner);
            let notifier = Arc::clone(&notifier);

            let handle = tokio::spawn(async move {
                let runner = match ChannelRunner::new(
                    channel_name.clone(),
                    channel_config,
                    scanner,
                    notifier,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Channel '{channel_name}' initialization failed: {e}");
                        return;
                    }
                };

                tokio::select! {
                    result = runner.run() => {
                        if let Err(e) = result {
                            error!("Channel '{channel_name}' failed: {e}");
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Channel '{channel_name}' received shutdown signal");
                    }
                }
            });

            handles.push(handle);
        }

        if handles.is_empty() {
            info!("No valid channels. Daemon waiting for shutdown signal...");
        } else {
            info!("All {} channels started. Daemon running...", handles.len());
        }

        wait_for_shutdown().await?;

        info!("Stopping all channels...");
        let _ = shutdown_tx.send(());

        // Wait for graceful shutdown with timeout
        let shutdown_future = futures::future::join_all(handles);
        tokio::select! {
            _ = shutdown_future => info!("All channels stopped gracefully"),
            () = sleep(SHUTDOWN_TIMEOUT) => {
                warn!("Shutdown timeout exceeded");
            }
        }

        info!("Daemon shutdown complete");
        Ok(())
    }
}

// =============================================================================
// Channel Runner
// =============================================================================

struct ChannelRunner {
    handler: ChannelHandler,
    watcher: Watcher,
}

impl ChannelRunner {
    fn new(
        name: String,
        config: ChannelConfig,
        scanner: Arc<dyn VirusScanner + Send + Sync>,
        notifier: Arc<Notifier>,
    ) -> Result<Self> {
        let debounce_duration = Duration::from_millis(config.debounce_ms);
        let watcher_config = WatcherConfig {
            debounce_duration,
            ..Default::default()
        };

        let mut watcher = Watcher::with_config(watcher_config)
            .context("Failed to create watcher")
            .map_err(|e| DaemonError::Channel {
                channel: name.clone(),
                message: e.to_string(),
                source: None,
            })?;

        // Add watches for each producer
        for producer in &config.producers {
            let producer_dir = config.base_path.join("share").join(producer);

            if !producer_dir.exists() {
                return Err(DaemonError::channel(
                    &name,
                    &format!("Share directory not found: {}", producer_dir.display()),
                    None::<std::io::Error>,
                )
                .into());
            }

            watcher.add_recursive(&producer_dir, producer)?;

            info!(
                "Channel '{}': monitoring {} (debounce={}ms)",
                &name,
                producer_dir.display(),
                debounce_duration.as_millis()
            );
        }

        let handler = ChannelHandler {
            name,
            config,
            scanner,
            notifier,
        };
        handler.verify_environment()?;

        Ok(Self { handler, watcher })
    }

    async fn run(mut self) -> Result<()> {
        info!("Starting channel '{}'", &self.handler.name);
        self.watcher.run(&mut self.handler).await;
        info!("Channel '{}' stopped", &self.handler.name);
        Ok(())
    }
}

// =============================================================================
// Channel Handler
// =============================================================================

struct ChannelHandler {
    name: String,
    config: ChannelConfig,
    scanner: Arc<dyn VirusScanner + Send + Sync>,
    notifier: Arc<Notifier>,
}

impl EventHandler for ChannelHandler {
    fn on_modified(&mut self, path: &Path, source: &str) -> Vec<(FileId, i64)> {
        let share_path = self.share_path(source);
        let Ok(rel) = path.strip_prefix(&share_path) else {
            warn!(
                "Channel '{}': {} not under {}, dropping",
                &self.name,
                path.display(),
                share_path.display()
            );
            return vec![];
        };
        let relative = rel.to_path_buf();

        if !Self::is_safe_relative_path(&relative) {
            warn!(
                "Channel '{}': path traversal in '{}', dropping",
                &self.name,
                relative.display()
            );
            return vec![];
        }
        if self.should_ignore(path, &relative) {
            debug!("Ignoring file: {}", path.display());
            return vec![];
        }

        let src_file = match Self::safe_open(path) {
            Ok(f) => f,
            Err(e) => {
                debug!("Skipping {}: {e}", path.display());
                return vec![];
            }
        };
        let src_meta = match src_file.metadata() {
            Ok(m) => m,
            Err(e) => {
                error!(
                    "Channel '{}': metadata failed for '{}': {e}",
                    &self.name,
                    relative.display()
                );
                return vec![];
            }
        };
        let tmp = match self.clone_to_staging(&src_file) {
            Ok(t) => t,
            Err(e) => {
                error!(
                    "Channel '{}': staging failed for '{}': {e}",
                    &self.name,
                    relative.display()
                );
                return vec![];
            }
        };

        info!(
            "Channel '{}': processing '{}' from {}",
            &self.name,
            relative.display(),
            source
        );

        // Scan staged snapshot - guarantees scanned bytes == published bytes
        let scan_result = match self.scanner.scan_fd(tmp.as_file().as_fd(), path) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Channel '{}': scan failed for '{}': {e}",
                    &self.name,
                    relative.display()
                );
                notify_error(&self.config.scanning.notify_socket, path, &e.to_string());
                if self.config.scanning.permissive {
                    ScanResult::Clean
                } else {
                    return vec![];
                }
            }
        };

        match scan_result {
            ScanResult::Clean => {
                debug!("Channel '{}': '{}' clean", &self.name, relative.display());
                let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
                self.spawn_notify();
                written
            }
            ScanResult::Infected(ref virus_name) => {
                warn!(
                    "Channel '{}': '{}' infected ({})",
                    &self.name,
                    relative.display(),
                    virus_name
                );
                notify_infected(&self.config.scanning.notify_socket, path, virus_name);
                self.handle_infected(&tmp, path, &relative);
                vec![]
            }
            ScanResult::Error if self.config.scanning.permissive => {
                warn!(
                    "Channel '{}': scan error for '{}' (permissive, treating as clean)",
                    &self.name,
                    relative.display()
                );
                let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
                self.spawn_notify();
                written
            }
            ScanResult::Error => {
                warn!(
                    "Channel '{}': scan error for '{}' (fail-safe, treating as infected)",
                    &self.name,
                    relative.display()
                );
                notify_error(
                    &self.config.scanning.notify_socket,
                    path,
                    "scan error (fail-safe)",
                );
                self.handle_infected(&tmp, path, &relative);
                vec![]
            }
            ScanResult::NotFound => {
                error!(
                    "Channel '{}': staged file lost for '{}' (unexpected)",
                    &self.name,
                    relative.display()
                );
                vec![]
            }
        }
    }

    fn on_deleted(&mut self, path: &Path, source: &str) {
        let share_path = self.share_path(source);
        let Ok(rel) = path.strip_prefix(&share_path) else {
            warn!(
                "Channel '{}': path {} not under share {}, dropping event",
                &self.name,
                path.display(),
                share_path.display()
            );
            return;
        };
        let relative = rel.to_path_buf();

        // Reject path traversal attempts
        if !Self::is_safe_relative_path(&relative) {
            warn!(
                "Channel '{}': path traversal in '{}', dropping event",
                &self.name,
                relative.display()
            );
            return;
        }

        // Check ignore patterns
        if self.should_ignore(path, &relative) {
            return;
        }

        // Delete from other producers if file exists (handles renames and synced deletes)
        for producer in &self.config.producers {
            if producer == source {
                continue;
            }
            let target = self.share_path(producer).join(&relative);
            if target.exists() {
                if let Err(e) = fs::remove_file(&target) {
                    debug!("Failed to delete {}: {e}", target.display());
                } else {
                    info!(
                        "Channel '{}': deleted '{}' from {}",
                        &self.name,
                        relative.display(),
                        producer
                    );
                }
            }
        }

        // Delete from export if consumers exist
        if !self.config.consumers.is_empty() {
            let export_target = self.export_path().join(&relative);
            if export_target.exists() {
                if let Err(e) = fs::remove_file(&export_target) {
                    debug!("Failed to delete {}: {e}", export_target.display());
                } else {
                    info!(
                        "Channel '{}': deleted '{}' from export",
                        &self.name,
                        relative.display()
                    );
                }
            }
        }

        // Notify guests about deletion
        self.spawn_notify();
    }

    fn on_renamed(&mut self, path: &Path, old_path: &Path, source: &str) -> Vec<(FileId, i64)> {
        let share_path = self.share_path(source);

        // Validate new path
        let Ok(rel) = path.strip_prefix(&share_path) else {
            warn!(
                "Channel '{}': {} not under {}, dropping",
                &self.name,
                path.display(),
                share_path.display()
            );
            return vec![];
        };
        let relative = rel.to_path_buf();

        // Validate old path
        let Ok(old_rel) = old_path.strip_prefix(&share_path) else {
            warn!(
                "Channel '{}': old path {} not under {}, dropping",
                &self.name,
                old_path.display(),
                share_path.display()
            );
            return vec![];
        };
        let old_relative = old_rel.to_path_buf();

        // Reject path traversal attempts
        if !Self::is_safe_relative_path(&relative) || !Self::is_safe_relative_path(&old_relative) {
            warn!(
                "Channel '{}': path traversal in rename, dropping",
                &self.name
            );
            return vec![];
        }

        if self.should_ignore(path, &relative) {
            debug!("Ignoring renamed file: {}", path.display());
            return vec![];
        }

        let src_file = match Self::safe_open(path) {
            Ok(f) => f,
            Err(e) => {
                debug!("Skipping renamed {}: {e}", path.display());
                return vec![];
            }
        };
        let src_meta = match src_file.metadata() {
            Ok(m) => m,
            Err(e) => {
                error!(
                    "Channel '{}': metadata failed for '{}': {e}",
                    &self.name,
                    relative.display()
                );
                return vec![];
            }
        };

        info!(
            "Channel '{}': rename '{}' -> '{}' from {}",
            &self.name,
            old_relative.display(),
            relative.display(),
            source
        );

        // Delete old path from other producers and export
        for producer in &self.config.producers {
            if producer == source {
                continue;
            }
            let old_target = self.share_path(producer).join(&old_relative);
            if old_target.exists() {
                if let Err(e) = fs::remove_file(&old_target) {
                    debug!("Failed to delete old {}: {e}", old_target.display());
                }
            }
        }
        if !self.config.consumers.is_empty() {
            let old_export = self.export_path().join(&old_relative);
            if old_export.exists() {
                let _ = fs::remove_file(&old_export);
            }
        }

        // Reflink new path to other producers and export (no scan - same content)
        let written = self.propagate_clean_from_file(&src_file, &src_meta, source, &relative);

        // Notify guests about rename
        self.spawn_notify();

        written
    }
}

impl ChannelHandler {
    /// Spawn a notification to guest VMs (fire-and-forget).
    fn spawn_notify(&self) {
        let notifier = Arc::clone(&self.notifier);
        let channel = self.name.clone();
        tokio::spawn(async move {
            notifier.notify(&channel).await;
        });
    }

    fn should_ignore(&self, path: &Path, relative: &Path) -> bool {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check filename patterns
        for pattern in &self.config.scanning.ignore_file_patterns {
            if filename.contains(pattern) {
                return true;
            }
        }

        // Check path patterns (against relative path)
        let relative_str = relative.to_string_lossy();
        for pattern in &self.config.scanning.ignore_path_patterns {
            if relative_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Check that a relative path has no traversal components (.. or absolute).
    fn is_safe_relative_path(path: &Path) -> bool {
        for component in path.components() {
            match component {
                Component::ParentDir | Component::RootDir => return false,
                _ => {}
            }
        }
        true
    }

    /// Safely open a file with security checks:
    /// - `O_RDONLY | O_NOFOLLOW | O_CLOEXEC`: reject symlinks, close on exec
    /// - Verify it's a regular file (not FIFO, device, etc.)
    /// - Verify it's not empty
    fn safe_open(path: &Path) -> Result<File> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| anyhow::anyhow!("open failed: {e}"))?;
        let file = File::from(fd);

        let metadata = file.metadata()?;
        if !metadata.file_type().is_file() {
            anyhow::bail!("not a regular file");
        }
        if metadata.len() == 0 {
            anyhow::bail!("empty file");
        }

        Ok(file)
    }

    /// Verify all environment requirements for operation.
    /// Checks: FICLONE support, `CAP_CHOWN` capability.
    fn verify_environment(&self) -> Result<()> {
        self.verify_reflink_support()?;
        self.verify_chown_capability()?;
        Ok(())
    }

    /// Verify that FICLONE (reflink) works from staging to all publish directories.
    fn verify_reflink_support(&self) -> Result<()> {
        let staging = self.staging_path();
        fs::create_dir_all(&staging)?;

        let src = NamedTempFile::new_in(&staging).context("failed to create test file")?;

        // Collect all directories we need to verify
        let mut targets: Vec<PathBuf> = Vec::new();

        // Producer share directories
        for producer in &self.config.producers {
            targets.push(self.share_path(producer));
        }

        // Export directory (if consumers exist)
        if !self.config.consumers.is_empty() {
            targets.push(self.export_path());
        }

        // Quarantine directory
        targets.push(self.quarantine_path());

        // Test FICLONE from staging to each target
        for target in &targets {
            fs::create_dir_all(target)?;
            let dst = NamedTempFile::new_in(target)
                .with_context(|| format!("failed to create test file in {}", target.display()))?;

            rustix::fs::ioctl_ficlone(dst.as_file(), src.as_file()).map_err(|e| {
                anyhow::anyhow!(
                    "Channel '{}': FICLONE from {} to {} failed (requires same btrfs/XFS): {e}",
                    &self.name,
                    staging.display(),
                    target.display()
                )
            })?;
        }

        info!(
            "Channel '{}': FICLONE verified for {} targets",
            &self.name,
            targets.len()
        );
        Ok(())
    }

    /// Verify we have `CAP_CHOWN` capability for preserving file ownership.
    fn verify_chown_capability(&self) -> Result<()> {
        let staging = self.staging_path();
        fs::create_dir_all(&staging)?;

        let tmp = NamedTempFile::new_in(&staging).context("failed to create test file")?;

        // Try to chown to uid/gid 0 - requires CAP_CHOWN
        let uid = rustix::fs::Uid::from_raw(0);
        let gid = rustix::fs::Gid::from_raw(0);

        rustix::fs::fchown(tmp.as_file(), Some(uid), Some(gid)).map_err(|e| {
            anyhow::anyhow!(
                "Channel '{}': CAP_CHOWN capability required for preserving file ownership: {e}",
                &self.name
            )
        })?;

        info!("Channel '{}': CAP_CHOWN verified", &self.name);
        Ok(())
    }

    /// Clone source file into staging directory.
    /// Returns a `NamedTempFile` containing a snapshot of the source.
    fn clone_to_staging(&self, src_file: &File) -> Result<NamedTempFile> {
        let staging = self.staging_path();
        fs::create_dir_all(&staging)?;

        let tmp = NamedTempFile::new_in(&staging).context("failed to create staging file")?;

        // FICLONE creates an atomic snapshot of extents - required for integrity
        rustix::fs::ioctl_ficlone(tmp.as_file(), src_file)
            .map_err(|e| anyhow::anyhow!("FICLONE failed (requires btrfs/XFS reflink): {e}"))?;

        Ok(tmp)
    }

    /// Propagate a clean staged file to other producers and export.
    /// Returns (`FileId`, ctime) pairs for written files (for loop prevention).
    fn propagate_clean(
        &self,
        tmp: &NamedTempFile,
        src_meta: &fs::Metadata,
        source_producer: &str,
        relative: &Path,
    ) -> Vec<(FileId, i64)> {
        let mut written = Vec::new();

        // Reflink to other producers
        for producer in &self.config.producers {
            if producer == source_producer {
                continue;
            }

            let target = self.share_path(producer).join(relative);
            if let Err(e) = Self::atomic_reflink(tmp.as_file(), Some(src_meta), &target) {
                error!(
                    "Channel '{}': failed to propagate '{}' to {}: {e}",
                    &self.name,
                    relative.display(),
                    producer
                );
            } else if let Ok(meta) = fs::metadata(&target) {
                let fid = (meta.dev(), meta.ino());
                let ctime = meta.ctime();
                written.push((fid, ctime));
            }
        }

        // Reflink to export (only if consumers exist)
        if !self.config.consumers.is_empty() {
            let export_target = self.export_path().join(relative);
            if let Err(e) = Self::atomic_reflink(tmp.as_file(), Some(src_meta), &export_target) {
                error!(
                    "Channel '{}': failed to export '{}': {e}",
                    &self.name,
                    relative.display()
                );
            }
        }

        debug!(
            "Channel '{}': propagated '{}'",
            &self.name,
            relative.display()
        );

        written
    }

    /// Propagate a file to other producers and export (for renames - no staging needed).
    /// Returns (`FileId`, ctime) pairs for written files (for loop prevention).
    fn propagate_clean_from_file(
        &self,
        src_file: &File,
        src_meta: &fs::Metadata,
        source_producer: &str,
        relative: &Path,
    ) -> Vec<(FileId, i64)> {
        let mut written = Vec::new();

        // Reflink to other producers
        for producer in &self.config.producers {
            if producer == source_producer {
                continue;
            }

            let target = self.share_path(producer).join(relative);
            if let Err(e) = Self::atomic_reflink(src_file, Some(src_meta), &target) {
                error!(
                    "Channel '{}': failed to propagate '{}' to {}: {e}",
                    &self.name,
                    relative.display(),
                    producer
                );
            } else if let Ok(meta) = fs::metadata(&target) {
                let fid = (meta.dev(), meta.ino());
                let ctime = meta.ctime();
                written.push((fid, ctime));
            }
        }

        // Reflink to export (only if consumers exist)
        if !self.config.consumers.is_empty() {
            let export_target = self.export_path().join(relative);
            if let Err(e) = Self::atomic_reflink(src_file, Some(src_meta), &export_target) {
                error!(
                    "Channel '{}': failed to export '{}': {e}",
                    &self.name,
                    relative.display()
                );
            }
        }

        debug!(
            "Channel '{}': propagated '{}' (rename)",
            &self.name,
            relative.display()
        );

        written
    }

    /// Atomically copy/reflink from an open file (TOCTOU-safe).
    ///
    /// Creates a temporary file, reflinks content into it, then atomically
    /// renames it to the destination. Overwrites existing files.
    ///
    /// If `src_meta` is `Some`, preserves permissions and ownership from the original.
    /// If `None`, sets root:root ownership with mode 000 (for quarantine).
    fn atomic_reflink(src_file: &File, src_meta: Option<&fs::Metadata>, dst: &Path) -> Result<()> {
        let parent = dst.parent().context("destination has no parent")?;
        fs::create_dir_all(parent)?;

        let tmp = NamedTempFile::new_in(parent).context("failed to create temp file")?;

        // FICLONE creates an atomic snapshot - required for integrity
        rustix::fs::ioctl_ficlone(tmp.as_file(), src_file)
            .map_err(|e| anyhow::anyhow!("FICLONE failed for {}: {e}", dst.display()))?;

        // Set permissions and ownership (None = root:root 000 for quarantine)
        // Mask out suid/sgid/sticky bits (0o7000) to prevent privilege escalation
        let (mode, uid, gid) =
            src_meta.map_or((0o000, 0, 0), |m| (m.mode() & 0o0777, m.uid(), m.gid()));

        let perms = fs::Permissions::from_mode(mode);
        tmp.as_file().set_permissions(perms)?;

        let uid = rustix::fs::Uid::from_raw(uid);
        let gid = rustix::fs::Gid::from_raw(gid);
        rustix::fs::fchown(tmp.as_file(), Some(uid), Some(gid))
            .with_context(|| format!("fchown failed for {} (requires CAP_CHOWN)", dst.display()))?;

        // Atomic rename to destination (overwrites if exists)
        tmp.persist(dst)
            .with_context(|| format!("failed to persist to {}", dst.display()))?;

        Ok(())
    }

    fn handle_infected(&self, tmp: &NamedTempFile, src_path: &Path, relative: &Path) {
        match self.config.scanning.infected_action {
            InfectedAction::Log => {
                // Infection already logged by on_modified, no further action
            }
            InfectedAction::Delete => {
                if let Err(e) = fs::remove_file(src_path) {
                    error!("Failed to delete infected file: {e}");
                } else {
                    info!(
                        "Channel '{}': deleted infected '{}'",
                        &self.name,
                        relative.display()
                    );
                }
            }
            InfectedAction::Quarantine => {
                let quarantine_dest = self.quarantine_path().join(relative);

                if let Err(e) = Self::atomic_reflink(tmp.as_file(), None, &quarantine_dest) {
                    warn!(
                        "Failed to quarantine '{}': {e} - deleting instead",
                        relative.display()
                    );
                } else {
                    info!(
                        "Channel '{}': quarantined '{}'",
                        &self.name,
                        relative.display()
                    );
                }
                let _ = fs::remove_file(src_path);
            }
        }
        // tmp is dropped here, removing the staged file
    }

    // =========================================================================
    // Path Helpers
    // =========================================================================

    fn share_path(&self, producer: &str) -> PathBuf {
        self.config.base_path.join("share").join(producer)
    }

    fn export_path(&self) -> PathBuf {
        self.config.base_path.join("export")
    }

    fn staging_path(&self) -> PathBuf {
        self.config.base_path.join("staging")
    }

    fn quarantine_path(&self) -> PathBuf {
        self.config.base_path.join("quarantine")
    }
}
