// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::{debug, info, warn};

use super::config::ChannelConfig;
use ghaf_virtiofs_tools::watcher::EventHandler;

// =============================================================================
// Types
// =============================================================================

/// File metadata from inventory scan.
#[derive(Debug, Clone)]
struct FileInfo {
    /// Full path to the file.
    path: PathBuf,
    /// Modification time (Unix timestamp).
    mtime: i64,
    /// File size in bytes.
    size: u64,
}

/// Inventory entry: source name and file info.
type InventoryEntry = (String, FileInfo);

/// Inventory: relative_path -> list of (source_name, FileInfo).
type Inventory = HashMap<PathBuf, Vec<InventoryEntry>>;

/// Action to take during sync.
#[derive(Debug)]
enum SyncAction {
    /// Trigger handler.on_modified() for this file.
    /// Handler will scan and propagate to other producers + export.
    TriggerSync {
        source: String,
        path: PathBuf,
        relative: PathBuf,
    },
    /// Delete orphan file from export (not in any producer).
    DeleteStale { path: PathBuf, relative: PathBuf },
}

/// Statistics from sync operation.
#[derive(Debug, Default)]
pub struct SyncStats {
    /// Files scanned in producers.
    pub producer_files: usize,
    /// Files scanned in export.
    pub export_files: usize,
    /// Files synced (triggered handler).
    pub synced: usize,
    /// Stale files deleted from export.
    pub deleted: usize,
    /// Errors encountered.
    pub errors: usize,
}

// =============================================================================
// Public API
// =============================================================================

/// Run startup sync for a channel.
///
/// Scans all producer directories and export, computes differences,
/// and triggers the handler to sync files. Returns statistics.
pub fn run<H: EventHandler>(
    name: &str,
    config: &ChannelConfig,
    handler: &mut H,
) -> Result<SyncStats> {
    info!("Channel '{name}': starting sync");

    // Build inventory
    let (producers, export) = build_inventory(config)?;

    let mut stats = SyncStats {
        producer_files: producers.values().map(|v| v.len()).sum(),
        export_files: export.values().map(|v| v.len()).sum(),
        ..Default::default()
    };

    debug!(
        "Channel '{name}': inventory complete (producers={}, export={})",
        stats.producer_files, stats.export_files
    );

    // Compute actions
    let actions = compute_actions(&producers, &export, config);
    debug!("Channel '{name}': {} sync actions", actions.len());

    // Execute actions
    for action in actions {
        match action {
            SyncAction::TriggerSync { source, path, relative } => {
                debug!("Channel '{name}': syncing '{}' from {}", relative.display(), source);
                let _written = handler.on_modified(&path, &source);
                stats.synced += 1;
            }
            SyncAction::DeleteStale { path, relative } => {
                debug!("Channel '{name}': deleting stale '{}'", relative.display());
                if let Err(e) = fs::remove_file(&path) {
                    warn!("Channel '{name}': failed to delete stale '{}': {e}", relative.display());
                    stats.errors += 1;
                } else {
                    stats.deleted += 1;
                }
            }
        }
    }

    info!(
        "Channel '{name}': sync complete (synced={}, deleted={}, errors={})",
        stats.synced, stats.deleted, stats.errors
    );

    Ok(stats)
}

// =============================================================================
// Inventory Building
// =============================================================================

/// Build inventory of all producers and export.
fn build_inventory(config: &ChannelConfig) -> Result<(Inventory, Inventory)> {
    let mut producers = Inventory::new();

    // Scan each producer directory
    for producer in &config.producers {
        let producer_dir = config.base_path.join("share").join(producer);
        if producer_dir.exists() {
            scan_directory(&producer_dir, &producer_dir, producer, &mut producers)?;
        }
    }

    // Scan export directory (only if consumers exist)
    let mut export = Inventory::new();
    if !config.consumers.is_empty() {
        let export_dir = config.base_path.join("export");
        if export_dir.exists() {
            scan_directory(&export_dir, &export_dir, "export", &mut export)?;
        }
    }

    Ok((producers, export))
}

/// Recursively scan a directory and add files to inventory.
fn scan_directory(
    root: &Path,
    dir: &Path,
    source: &str,
    inventory: &mut Inventory,
) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let ft = meta.file_type();

        if ft.is_symlink() {
            continue;
        }

        if ft.is_dir() {
            scan_directory(root, &path, source, inventory)?;
        } else if ft.is_file() {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };

            let info = FileInfo {
                path: path.clone(),
                mtime: meta.mtime(),
                size: meta.size(),
            };

            inventory.entry(relative.to_path_buf())
                .or_default()
                .push((source.to_string(), info));
        }
    }

    Ok(())
}

// =============================================================================
// Action Computation
// =============================================================================

/// Compute sync actions from inventories.
fn compute_actions(
    producers: &Inventory,
    export: &Inventory,
    config: &ChannelConfig,
) -> Vec<SyncAction> {
    let mut actions = Vec::new();

    // Process each file in producers
    for (relative, entries) in producers {
        if let Some(action) = compute_producer_action(relative, entries, export, config) {
            actions.push(action);
        }
    }

    // Find stale files in export (not in any producer)
    for (relative, entries) in export {
        if !producers.contains_key(relative) {
            if let Some((_, info)) = entries.first() {
                actions.push(SyncAction::DeleteStale {
                    path: info.path.clone(),
                    relative: relative.clone(),
                });
            }
        }
    }

    actions
}

/// Compute action for a file that exists in producer(s).
fn compute_producer_action(
    relative: &Path,
    entries: &[InventoryEntry],
    export: &Inventory,
    config: &ChannelConfig,
) -> Option<SyncAction> {
    if entries.is_empty() {
        return None;
    }

    // Find the entry with latest mtime (conflict resolution)
    let (best_source, best_info) = entries
        .iter()
        .max_by_key(|(_, info)| info.mtime)
        .map(|(s, i)| (s.clone(), i.clone()))?;

    // Check if producers are in conflict
    let producers_in_conflict = entries.len() > 1 && {
        let first = &entries[0].1;
        entries.iter().skip(1).any(|(_, info)| {
            info.mtime != first.mtime || info.size != first.size
        })
    };

    // Check if file needs to be synced to other producers
    let needs_producer_sync = entries.len() < config.producers.len();

    // Check if file needs to be synced to export
    let needs_export_sync = if config.consumers.is_empty() {
        false
    } else {
        match export.get(relative) {
            None => true, // Not in export
            Some(export_entries) => {
                // In export but different content
                export_entries.iter().any(|(_, info)| {
                    info.mtime != best_info.mtime || info.size != best_info.size
                })
            }
        }
    };

    // Trigger sync if:
    // - Producers are in conflict (need to resolve)
    // - File missing from some producers
    // - File missing or outdated in export
    if producers_in_conflict || needs_producer_sync || needs_export_sync {
        Some(SyncAction::TriggerSync {
            source: best_source,
            path: best_info.path,
            relative: relative.to_path_buf(),
        })
    } else {
        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_resolution_picks_latest_mtime() {
        let mut producers = Inventory::new();
        let relative = PathBuf::from("file.txt");

        producers.insert(
            relative.clone(),
            vec![
                (
                    "vm1".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm1/file.txt"),
                        mtime: 100,
                        size: 1000,
                    },
                ),
                (
                    "vm2".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm2/file.txt"),
                        mtime: 200, // Latest
                        size: 1000,
                    },
                ),
            ],
        );

        let export = Inventory::new();
        let config = make_test_config(vec!["vm1", "vm2"], vec!["consumer"]);

        let actions = compute_actions(&producers, &export, &config);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            SyncAction::TriggerSync { source, .. } => {
                assert_eq!(source, "vm2"); // Should pick vm2 (latest mtime)
            }
            _ => panic!("Expected TriggerSync"),
        }
    }

    #[test]
    fn test_stale_export_file_deleted() {
        let producers = Inventory::new(); // No producers have this file

        let mut export = Inventory::new();
        export.insert(
            PathBuf::from("orphan.txt"),
            vec![(
                "export".to_string(),
                FileInfo {
                    path: PathBuf::from("/export/orphan.txt"),
                    mtime: 100,
                    size: 500,
                },
            )],
        );

        let config = make_test_config(vec!["vm1"], vec!["consumer"]);

        let actions = compute_actions(&producers, &export, &config);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            SyncAction::DeleteStale { relative, .. } => {
                assert_eq!(relative, &PathBuf::from("orphan.txt"));
            }
            _ => panic!("Expected DeleteStale"),
        }
    }

    #[test]
    fn test_file_in_sync_no_action() {
        let mut producers = Inventory::new();
        let relative = PathBuf::from("synced.txt");

        // Same file in both producers with identical metadata
        producers.insert(
            relative.clone(),
            vec![
                (
                    "vm1".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm1/synced.txt"),
                        mtime: 100,
                        size: 1000,
                    },
                ),
                (
                    "vm2".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm2/synced.txt"),
                        mtime: 100,
                        size: 1000,
                    },
                ),
            ],
        );

        // Same file in export with identical metadata
        let mut export = Inventory::new();
        export.insert(
            relative.clone(),
            vec![(
                "export".to_string(),
                FileInfo {
                    path: PathBuf::from("/export/synced.txt"),
                    mtime: 100,
                    size: 1000,
                },
            )],
        );

        let config = make_test_config(vec!["vm1", "vm2"], vec!["consumer"]);

        let actions = compute_actions(&producers, &export, &config);

        assert!(actions.is_empty(), "No action needed for synced files");
    }

    #[test]
    fn test_file_missing_from_export() {
        let mut producers = Inventory::new();
        let relative = PathBuf::from("new.txt");

        producers.insert(
            relative.clone(),
            vec![(
                "vm1".to_string(),
                FileInfo {
                    path: PathBuf::from("/share/vm1/new.txt"),
                    mtime: 100,
                    size: 1000,
                },
            )],
        );

        let export = Inventory::new(); // File not in export

        let config = make_test_config(vec!["vm1"], vec!["consumer"]);

        let actions = compute_actions(&producers, &export, &config);

        assert_eq!(actions.len(), 1);
        matches!(&actions[0], SyncAction::TriggerSync { .. });
    }

    #[test]
    fn test_no_export_sync_without_consumers() {
        let mut producers = Inventory::new();
        let relative = PathBuf::from("file.txt");

        // File in both producers, in sync
        producers.insert(
            relative.clone(),
            vec![
                (
                    "vm1".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm1/file.txt"),
                        mtime: 100,
                        size: 1000,
                    },
                ),
                (
                    "vm2".to_string(),
                    FileInfo {
                        path: PathBuf::from("/share/vm2/file.txt"),
                        mtime: 100,
                        size: 1000,
                    },
                ),
            ],
        );

        let export = Inventory::new();

        // No consumers - export sync not needed
        let config = make_test_config(vec!["vm1", "vm2"], vec![]);

        let actions = compute_actions(&producers, &export, &config);

        assert!(actions.is_empty(), "No export sync without consumers");
    }

    #[test]
    fn test_file_missing_from_some_producers() {
        let mut producers = Inventory::new();
        let relative = PathBuf::from("partial.txt");

        // File only in vm1, not in vm2
        producers.insert(
            relative.clone(),
            vec![(
                "vm1".to_string(),
                FileInfo {
                    path: PathBuf::from("/share/vm1/partial.txt"),
                    mtime: 100,
                    size: 1000,
                },
            )],
        );

        let export = Inventory::new();

        // Two producers configured
        let config = make_test_config(vec!["vm1", "vm2"], vec!["consumer"]);

        let actions = compute_actions(&producers, &export, &config);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            SyncAction::TriggerSync { source, .. } => {
                assert_eq!(source, "vm1");
            }
            _ => panic!("Expected TriggerSync"),
        }
    }

    fn make_test_config(producers: Vec<&str>, consumers: Vec<&str>) -> ChannelConfig {
        use super::super::config::ScanningConfig;

        ChannelConfig {
            base_path: PathBuf::from("/tmp/test"),
            producers: producers.into_iter().map(String::from).collect(),
            consumers: consumers.into_iter().map(String::from).collect(),
            debounce_ms: 1000,
            scanning: ScanningConfig::default(),
            notify: None,
        }
    }
}
