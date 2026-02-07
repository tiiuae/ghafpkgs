// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File};
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use rustix::fs::{Mode, OFlags};
use tempfile::NamedTempFile;
use tokio::time::sleep;

use super::config::{ChannelConfig, Config};
use super::notify::{Notifier, build_notifier};
use ghaf_virtiofs_tools::scanner::{ScanResult, VirusScanner};
use ghaf_virtiofs_tools::util::{InfectedAction, notify_error, notify_infected, wait_for_shutdown};
use ghaf_virtiofs_tools::watcher::{EventHandler, FileId, Watcher, WatcherConfig};

// =============================================================================
// Constants
// =============================================================================

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_CHANNEL_CAPACITY: usize = 1;

// =============================================================================
// Daemon
// =============================================================================

pub struct Daemon {
    config: Config,
    scanner: Arc<dyn VirusScanner + Send + Sync>,
}

/// Main daemon entry point: spawns channel runners and handles shutdown.
impl Daemon {
    pub fn new(config: Config, scanner: Arc<dyn VirusScanner + Send + Sync>) -> Self {
        Self { config, scanner }
    }

    pub async fn run(self) -> Result<()> {
        info!("virtiofs-gate: starting ({} channels)", self.config.len());

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
                        debug!("Channel '{channel_name}' received shutdown signal");
                    }
                }
            });

            handles.push(handle);
        }

        if handles.is_empty() {
            warn!("virtiofs-gate: no valid channels, waiting for shutdown");
        } else {
            info!("virtiofs-gate: ready ({} channels)", handles.len());
        }

        wait_for_shutdown().await?;

        debug!("Stopping all channels...");
        let _ = shutdown_tx.send(());

        // Wait for graceful shutdown with timeout
        let shutdown_future = futures::future::join_all(handles);
        tokio::select! {
            _ = shutdown_future => debug!("All channels stopped gracefully"),
            () = sleep(SHUTDOWN_TIMEOUT) => {
                warn!("Shutdown timeout exceeded");
            }
        }

        info!("virtiofs-gate: stopped");
        Ok(())
    }
}

// =============================================================================
// Channel Runner
// =============================================================================

struct ChannelRunner {
    name: String,
    config: ChannelConfig,
    handler: ChannelHandler,
}

/// Per-channel watcher lifecycle: sync, setup, and event loop.
impl ChannelRunner {
    fn new(
        name: String,
        config: ChannelConfig,
        scanner: Arc<dyn VirusScanner + Send + Sync>,
        notifier: Arc<Notifier>,
    ) -> Result<Self> {
        let handler = ChannelHandler {
            name: name.clone(),
            config: config.clone(),
            scanner,
            notifier,
        };
        handler.verify_environment()?;

        Ok(Self {
            name,
            config,
            handler,
        })
    }

    async fn run(mut self) -> Result<()> {
        debug!("Channel '{}': started", self.name);

        // Run startup sync before creating watcher (no events captured)
        if let Err(e) = super::sync::run(&self.name, &self.config, &mut self.handler) {
            warn!("Channel '{}': sync failed: {e}", self.name);
        }

        // Create watcher after sync completes
        let debounce_duration = Duration::from_millis(self.config.debounce_ms);
        let watcher_config = WatcherConfig {
            debounce_duration,
            ..Default::default()
        };

        let mut watcher = Watcher::with_config(watcher_config)
            .with_context(|| format!("Channel '{}': failed to create watcher", self.name))?;

        // Add watches for each producer
        for producer in &self.config.producers {
            let producer_dir = self.config.base_path.join("share").join(producer);

            if !producer_dir.exists() {
                anyhow::bail!(
                    "Channel '{}': share directory not found: {}",
                    self.name,
                    producer_dir.display()
                );
            }

            watcher.add_recursive(&producer_dir, producer)?;
            info!(
                "Channel '{}': monitoring {} (debounce={}ms)",
                self.name,
                producer_dir.display(),
                debounce_duration.as_millis()
            );
        }

        // Run watcher event loop
        watcher.run(&mut self.handler).await;
        debug!("Channel '{}': stopped", self.name);
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

/// Implements `Watcher`'s `EventHandler` trait for inotify events: modify, delete, rename.
impl EventHandler for ChannelHandler {
    #[allow(clippy::too_many_lines)]
    fn on_modified(&mut self, path: &Path, source: &str) -> Vec<(FileId, i64)> {
        let share_path = self.share_path(source);
        let Ok(rel) = path.strip_prefix(&share_path) else {
            warn!(
                "Channel '{}': {} not under {}, dropping",
                self.name,
                path.display(),
                share_path.display()
            );
            return vec![];
        };
        let relative = rel.to_path_buf();

        // Sanity checks
        if !Self::is_safe_relative_path(&relative) {
            warn!(
                "Channel '{}': path traversal in '{}', dropping",
                self.name,
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
                    self.name,
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
                    self.name,
                    relative.display()
                );
                return vec![];
            }
        };

        info!(
            "Channel '{}': processing '{}' from {}",
            self.name,
            relative.display(),
            source
        );

        // Empty files bypass scan (no content to scan, may be marker files)
        if src_meta.len() == 0 {
            debug!(
                "Channel '{}': '{}' empty, skipping scan",
                self.name,
                relative.display()
            );
            let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
            self.spawn_notify();
            return written;
        }

        // Skip scanning if disabled for this channel
        if !self.config.scanning.enabled {
            debug!(
                "Channel '{}': '{}' scanning disabled, treating as clean",
                self.name,
                relative.display()
            );
            let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
            self.spawn_notify();
            return written;
        }

        // Scan staged snapshot - guarantees scanned bytes == published bytes
        let scan_result = match self.scanner.scan_fd(tmp.as_file().as_fd(), path) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Channel '{}': scan failed for '{}': {e}",
                    self.name,
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
                debug!("Channel '{}': '{}' clean", self.name, relative.display());
                let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
                self.spawn_notify();
                written
            }
            ScanResult::Infected(virus_name) => {
                warn!(
                    "Channel '{}': '{}' infected ({})",
                    self.name,
                    relative.display(),
                    &virus_name
                );
                notify_infected(&self.config.scanning.notify_socket, path, &virus_name);
                self.handle_infected(&tmp, path, &relative);
                vec![]
            }
            ScanResult::Error if self.config.scanning.permissive => {
                warn!(
                    "Channel '{}': scan error for '{}' (permissive, treating as clean)",
                    self.name,
                    relative.display()
                );
                let written = self.propagate_clean(&tmp, &src_meta, source, &relative);
                self.spawn_notify();
                written
            }
            ScanResult::Error => {
                warn!(
                    "Channel '{}': scan error for '{}' (fail-safe, treating as infected)",
                    self.name,
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
                    self.name,
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
                "Channel '{}': path {} not under share {}, dropping",
                self.name,
                path.display(),
                share_path.display()
            );
            return;
        };
        let relative = rel.to_path_buf();

        // Sanity checks
        if !Self::is_safe_relative_path(&relative) {
            warn!(
                "Channel '{}': path traversal in '{}', dropping",
                self.name,
                relative.display()
            );
            return;
        }
        if self.should_ignore(path, &relative) {
            return;
        }

        // Delete from other producers if file exists (skip diode producers)
        for producer in &self.config.producers {
            if producer == source || self.config.is_diode(producer) {
                continue;
            }
            let target = self.share_path(producer).join(&relative);
            if target.exists() {
                if let Err(e) = fs::remove_file(&target) {
                    debug!("Failed to delete {}: {e}", target.display());
                } else {
                    info!(
                        "Channel '{}': deleted '{}' from {}",
                        self.name,
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
                        self.name,
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
                self.name,
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
                self.name,
                old_path.display(),
                share_path.display()
            );
            return vec![];
        };
        let old_relative = old_rel.to_path_buf();

        // Sanity checks
        if !Self::is_safe_relative_path(&relative) || !Self::is_safe_relative_path(&old_relative) {
            warn!(
                "Channel '{}': path traversal in rename, dropping",
                self.name
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
                    self.name,
                    relative.display()
                );
                return vec![];
            }
        };

        info!(
            "Channel '{}': rename '{}' -> '{}' from {}",
            self.name,
            old_relative.display(),
            relative.display(),
            source
        );

        // Delete old path from other producers and export (skip diode producers)
        for producer in &self.config.producers {
            if producer == source || self.config.is_diode(producer) {
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

/// File processing: scanning, propagation, and path helpers.
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
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let relative_str = relative.to_string_lossy();
        self.config
            .scanning
            .ignore_file_patterns
            .iter()
            .any(|p| filename.contains(p))
            || self
                .config
                .scanning
                .ignore_path_patterns
                .iter()
                .any(|p| relative_str.contains(p.as_str()))
    }

    /// Check that a relative path has no traversal components (.. or absolute).
    fn is_safe_relative_path(path: &Path) -> bool {
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
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

        Ok(file)
    }

    /// Verify environment requirements for operation.
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
                    self.name,
                    staging.display(),
                    target.display()
                )
            })?;
        }

        info!(
            "Channel '{}': FICLONE verified for {} targets",
            self.name,
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
                self.name
            )
        })?;

        info!("Channel '{}': CAP_CHOWN verified", self.name);
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
        src_producer: &str,
        src_relative: &Path,
    ) -> Vec<(FileId, i64)> {
        let mut written = Vec::new();
        let is_diode_source = self.config.is_diode(src_producer);

        // Reflink to other producers (skip diode producers)
        for producer in &self.config.producers {
            if producer == src_producer || self.config.is_diode(producer) {
                continue;
            }

            let target = self.share_path(producer).join(src_relative);

            // Diode ignore-existing: skip if target already exists
            if is_diode_source && target.exists() {
                debug!(
                    "Channel '{}': diode skip existing '{}' in {}",
                    self.name,
                    src_relative.display(),
                    producer
                );
                continue;
            }

            if let Err(e) = Self::atomic_reflink(tmp.as_file(), Some(src_meta), &target) {
                error!(
                    "Channel '{}': failed to propagate '{}' to {}: {e}",
                    self.name,
                    src_relative.display(),
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
            let export_target = self.export_path().join(src_relative);

            // Diode ignore-existing: skip if export target already exists
            if is_diode_source && export_target.exists() {
                debug!(
                    "Channel '{}': diode skip existing '{}' in export",
                    self.name,
                    src_relative.display()
                );
            } else if let Err(e) =
                Self::atomic_reflink(tmp.as_file(), Some(src_meta), &export_target)
            {
                error!(
                    "Channel '{}': failed to export '{}': {e}",
                    self.name,
                    src_relative.display()
                );
            }
        }

        debug!(
            "Channel '{}': propagated '{}'",
            self.name,
            src_relative.display()
        );
        written
    }

    /// Propagate a file to other producers and export (for renames - no staging needed).
    /// Returns (`FileId`, ctime) pairs for written files (for loop prevention).
    fn propagate_clean_from_file(
        &self,
        src_file: &File,
        src_meta: &fs::Metadata,
        src_producer: &str,
        src_relative: &Path,
    ) -> Vec<(FileId, i64)> {
        let mut written = Vec::new();
        let is_diode_source = self.config.is_diode(src_producer);

        // Reflink to other producers (skip diode producers)
        for producer in &self.config.producers {
            if producer == src_producer || self.config.is_diode(producer) {
                continue;
            }

            let target = self.share_path(producer).join(src_relative);

            // Diode ignore-existing: skip if target already exists
            if is_diode_source && target.exists() {
                debug!(
                    "Channel '{}': diode skip existing '{}' in {}",
                    self.name,
                    src_relative.display(),
                    producer
                );
                continue;
            }

            if let Err(e) = Self::atomic_reflink(src_file, Some(src_meta), &target) {
                error!(
                    "Channel '{}': failed to propagate '{}' to {}: {e}",
                    self.name,
                    src_relative.display(),
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
            let export_target = self.export_path().join(src_relative);

            // Diode ignore-existing: skip if export target already exists
            if is_diode_source && export_target.exists() {
                debug!(
                    "Channel '{}': diode skip existing '{}' in export",
                    self.name,
                    src_relative.display()
                );
            } else if let Err(e) = Self::atomic_reflink(src_file, Some(src_meta), &export_target) {
                error!(
                    "Channel '{}': failed to export '{}': {e}",
                    self.name,
                    src_relative.display()
                );
            }
        }

        debug!(
            "Channel '{}': propagated '{}' (rename)",
            self.name,
            src_relative.display()
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
        // Mask out suid/sgid/sticky bits (0o7000)
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
            InfectedAction::Log => {} // Infection already logged by on_modified
            InfectedAction::Delete => {
                if let Err(e) = fs::remove_file(src_path) {
                    error!("Failed to delete infected file: {e}");
                } else {
                    info!(
                        "Channel '{}': deleted infected '{}'",
                        self.name,
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
                        self.name,
                        relative.display()
                    );
                }
                let _ = fs::remove_file(src_path);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // =========================================================================
    // is_safe_relative_path tests
    // =========================================================================

    #[test]
    fn safe_path_simple() {
        assert!(ChannelHandler::is_safe_relative_path(Path::new("file.txt")));
    }

    #[test]
    fn safe_path_nested() {
        assert!(ChannelHandler::is_safe_relative_path(Path::new(
            "subdir/file.txt"
        )));
    }

    #[test]
    fn safe_path_deeply_nested() {
        assert!(ChannelHandler::is_safe_relative_path(Path::new(
            "a/b/c/d/file.txt"
        )));
    }

    #[test]
    fn unsafe_path_parent_simple() {
        assert!(!ChannelHandler::is_safe_relative_path(Path::new(
            "../file.txt"
        )));
    }

    #[test]
    fn unsafe_path_parent_nested() {
        assert!(!ChannelHandler::is_safe_relative_path(Path::new(
            "subdir/../file.txt"
        )));
    }

    #[test]
    fn unsafe_path_parent_deep() {
        assert!(!ChannelHandler::is_safe_relative_path(Path::new(
            "a/b/../../c/file.txt"
        )));
    }

    #[test]
    fn unsafe_path_absolute() {
        assert!(!ChannelHandler::is_safe_relative_path(Path::new(
            "/etc/passwd"
        )));
    }

    #[test]
    fn unsafe_path_absolute_nested() {
        assert!(!ChannelHandler::is_safe_relative_path(Path::new(
            "/var/log/file.txt"
        )));
    }

    #[test]
    fn safe_path_dotfile() {
        // Single dot in filename is fine
        assert!(ChannelHandler::is_safe_relative_path(Path::new(".hidden")));
    }

    #[test]
    fn safe_path_dot_in_name() {
        // Dots in middle of filename are fine
        assert!(ChannelHandler::is_safe_relative_path(Path::new(
            "file..txt"
        )));
    }

    #[test]
    fn safe_path_empty() {
        // Empty path has no dangerous components
        assert!(ChannelHandler::is_safe_relative_path(Path::new("")));
    }

    // =========================================================================
    // should_ignore tests
    // =========================================================================

    fn make_handler_with_patterns(
        file_patterns: Vec<&str>,
        path_patterns: Vec<&str>,
    ) -> ChannelHandler {
        use super::super::config::{ChannelConfig, ScanningConfig};
        use ghaf_virtiofs_tools::scanner::NoopScanner;
        use std::sync::Arc;

        let config = ChannelConfig {
            base_path: PathBuf::from("/tmp/test"),
            producers: vec!["vm1".to_string()],
            consumers: vec![],
            diode_producers: vec![],
            debounce_ms: 1000,
            scanning: ScanningConfig {
                ignore_file_patterns: file_patterns.into_iter().map(String::from).collect(),
                ignore_path_patterns: path_patterns.into_iter().map(String::from).collect(),
                ..Default::default()
            },
            notify: None,
        };

        ChannelHandler {
            name: "test".to_string(),
            config,
            scanner: Arc::new(NoopScanner),
            notifier: Arc::new(super::super::notify::Notifier::disabled()),
        }
    }

    #[test]
    fn ignore_crdownload() {
        let handler = make_handler_with_patterns(vec![".crdownload"], vec![]);
        assert!(handler.should_ignore(
            Path::new("/share/vm1/file.crdownload"),
            Path::new("file.crdownload")
        ));
    }

    #[test]
    fn ignore_part_file() {
        let handler = make_handler_with_patterns(vec![".part"], vec![]);
        assert!(handler.should_ignore(
            Path::new("/share/vm1/download.part"),
            Path::new("download.part")
        ));
    }

    #[test]
    fn ignore_tilde_prefix() {
        let handler = make_handler_with_patterns(vec!["~$"], vec![]);
        assert!(handler.should_ignore(
            Path::new("/share/vm1/~$document.docx"),
            Path::new("~$document.docx")
        ));
    }

    #[test]
    fn ignore_tmp_extension() {
        let handler = make_handler_with_patterns(vec![".tmp"], vec![]);
        assert!(handler.should_ignore(Path::new("/share/vm1/file.tmp"), Path::new("file.tmp")));
    }

    #[test]
    fn no_ignore_normal_file() {
        let handler = make_handler_with_patterns(vec![".crdownload", ".part", ".tmp"], vec![]);
        assert!(!handler.should_ignore(
            Path::new("/share/vm1/document.pdf"),
            Path::new("document.pdf")
        ));
    }

    #[test]
    fn no_ignore_similar_name() {
        let handler = make_handler_with_patterns(vec![".crdownload"], vec![]);
        // "crdownload" without the dot should not match ".crdownload" pattern
        assert!(
            !handler.should_ignore(Path::new("/share/vm1/crdownload"), Path::new("crdownload"))
        );
    }

    #[test]
    fn ignore_trash_path() {
        let handler = make_handler_with_patterns(vec![], vec![".Trash-"]);
        assert!(handler.should_ignore(
            Path::new("/share/vm1/.Trash-1000/file.txt"),
            Path::new(".Trash-1000/file.txt")
        ));
    }

    #[test]
    fn ignore_local_trash_path() {
        let handler = make_handler_with_patterns(vec![], vec![".local/share/Trash"]);
        assert!(handler.should_ignore(
            Path::new("/share/vm1/.local/share/Trash/files/deleted.txt"),
            Path::new(".local/share/Trash/files/deleted.txt")
        ));
    }

    #[test]
    fn no_ignore_normal_path() {
        let handler = make_handler_with_patterns(vec![], vec![".Trash-"]);
        assert!(!handler.should_ignore(
            Path::new("/share/vm1/documents/file.txt"),
            Path::new("documents/file.txt")
        ));
    }

    #[test]
    fn ignore_combined_patterns() {
        let handler = make_handler_with_patterns(vec![".crdownload"], vec![".Trash-"]);
        // File pattern match
        assert!(handler.should_ignore(
            Path::new("/share/vm1/file.crdownload"),
            Path::new("file.crdownload")
        ));
        // Path pattern match
        assert!(handler.should_ignore(
            Path::new("/share/vm1/.Trash-1000/file.txt"),
            Path::new(".Trash-1000/file.txt")
        ));
        // Neither match
        assert!(
            !handler.should_ignore(Path::new("/share/vm1/normal.txt"), Path::new("normal.txt"))
        );
    }

    #[test]
    fn ignore_empty_patterns() {
        let handler = make_handler_with_patterns(vec![], vec![]);
        assert!(!handler.should_ignore(Path::new("/share/vm1/file.txt"), Path::new("file.txt")));
    }

    #[test]
    fn ignore_pattern_in_middle() {
        let handler = make_handler_with_patterns(vec![".part"], vec![]);
        // Pattern can match anywhere in filename
        assert!(handler.should_ignore(
            Path::new("/share/vm1/file.part.bak"),
            Path::new("file.part.bak")
        ));
    }
}
