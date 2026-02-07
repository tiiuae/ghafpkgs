// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

use ghaf_virtiofs_tools::util::{InfectedAction, REFRESH_TRIGGER_FILE};

/// Scanning configuration for a channel.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ScanningConfig {
    /// Enable virus scanning for this channel (default: true).
    /// Set to false to skip scanning and treat all files as clean.
    #[serde(default = "default_enable")]
    pub enable: bool,

    /// Action to take on infected files: log, delete (default), or quarantine.
    #[serde(default, rename = "infectedAction")]
    pub infected_action: InfectedAction,

    /// Permissive mode: treat scan errors as clean (true) or infected (false).
    /// Default is false (fail-safe: errors are treated as infected).
    #[serde(default)]
    pub permissive: bool,

    /// Filename patterns to ignore (temp files that shouldn't be scanned).
    /// Matches against the filename only.
    /// Examples: `.crdownload`, `.part`, `.tmp`, `~$`
    #[serde(default, rename = "ignoreFilePatterns")]
    pub ignore_file_patterns: Vec<String>,

    /// Path patterns to ignore (system directories that shouldn't be synced).
    /// Matches against the full relative path.
    /// Examples: `.Trash-`, `.local/share/Trash`
    #[serde(default, rename = "ignorePathPatterns")]
    pub ignore_path_patterns: Vec<String>,
}

const fn default_enable() -> bool {
    true
}

impl Default for ScanningConfig {
    fn default() -> Self {
        Self {
            enable: true,
            infected_action: InfectedAction::default(),
            permissive: false,
            ignore_file_patterns: Vec::new(),
            ignore_path_patterns: Vec::new(),
        }
    }
}

/// User notification configuration for malware alerts.
///
/// Controls desktop notifications sent when malware is detected or scan errors occur.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserNotifyConfig {
    /// Enable user notifications for this channel (default: true).
    /// Set to false to disable desktop notifications.
    /// Notifications are also skipped if the socket doesn't exist.
    #[serde(default = "default_enable")]
    pub enable: bool,

    /// Socket path for user notifications.
    /// Default: /run/clamav/notify.sock
    #[serde(default = "default_user_notify_socket")]
    pub socket: PathBuf,
}

fn default_user_notify_socket() -> PathBuf {
    PathBuf::from("/run/clamav/notify.sock")
}

impl Default for UserNotifyConfig {
    fn default() -> Self {
        Self {
            enable: true,
            socket: default_user_notify_socket(),
        }
    }
}

/// Default vsock port for guest notifications
const fn default_notify_port() -> u32 {
    3401
}

/// Default debounce duration in milliseconds
const fn default_debounce_ms() -> u64 {
    1000
}

/// Configuration for guest VM notifications (file browser refresh via vsock).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GuestNotifyConfig {
    /// Guest VM CIDs to notify about file changes
    #[serde(default)]
    pub guests: Vec<u32>,

    /// Vsock port for notifications (default 3401)
    #[serde(default = "default_notify_port")]
    pub port: u32,
}

/// Configuration for a single sharing channel.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChannelConfig {
    /// Base path containing share/, export/, staging/, quarantine/
    #[serde(rename = "basePath")]
    pub base_path: PathBuf,

    /// Producer VMs that write files to this channel (bidirectional sharing)
    pub producers: Vec<String>,

    /// Consumer VMs that read files from this channel (read-only via export-ro/)
    pub consumers: Vec<String>,

    /// Producers operating in diode mode (write-only, no sync from others).
    /// Must be a subset of `producers`.
    #[serde(default, rename = "diodeProducers")]
    pub diode_producers: Vec<String>,

    /// Debounce duration in milliseconds (default: 1000ms).
    /// Wait this long after the last write before processing a file.
    #[serde(default = "default_debounce_ms", rename = "debounceMs")]
    pub debounce_ms: u64,

    /// Scanning configuration
    #[serde(default)]
    pub scanning: ScanningConfig,

    /// User notification configuration for malware alerts
    #[serde(default, rename = "userNotify")]
    pub user_notify: UserNotifyConfig,

    /// Guest VM notification configuration (file browser refresh via vsock)
    #[serde(default, rename = "guestNotify")]
    pub guest_notify: Option<GuestNotifyConfig>,
}

/// Map of channel name to configuration.
pub type Config = HashMap<String, ChannelConfig>;

impl ChannelConfig {
    /// Validate channel configuration.
    /// Returns `Ok(())` if valid, or `Err(Vec<String>)` with error messages.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors: Vec<String> = Vec::new();

        // Validate base path exists and is a directory
        if !self.base_path.exists() {
            errors.push("Base path does not exist".to_string());
        } else if !self.base_path.is_dir() {
            errors.push("Base path is not a directory".to_string());
        }

        // Validate share directory exists
        let share = self.base_path.join("share");
        if !share.exists() {
            errors.push("Required 'share' directory does not exist".to_string());
        } else if !share.is_dir() {
            errors.push("'share' path exists but is not a directory".to_string());
        }

        // Validate export directory exists (only required if consumers exist)
        if !self.consumers.is_empty() {
            let export = self.base_path.join("export");
            if !export.exists() {
                errors.push("Required 'export' directory does not exist".to_string());
            } else if !export.is_dir() {
                errors.push("'export' path exists but is not a directory".to_string());
            }
        }

        // Validate per-producer share directories exist
        for producer in &self.producers {
            let producer_dir = share.join(producer);
            if !producer_dir.exists() {
                errors.push(format!("'share/{producer}' does not exist"));
            } else if !producer_dir.is_dir() {
                errors.push(format!("'share/{producer}' exists but is not a directory"));
            }
        }

        // Validate quarantine directory if quarantine action is configured
        if self.scanning.infected_action == InfectedAction::Quarantine {
            let quarantine = self.base_path.join("quarantine");
            if !quarantine.exists() {
                errors.push(
                    "Required 'quarantine' directory does not exist (infectedAction=quarantine)"
                        .to_string(),
                );
            } else if !quarantine.is_dir() {
                errors.push("'quarantine' path exists but is not a directory".to_string());
            }
        }

        // Validate producers exist
        if self.producers.is_empty() {
            errors.push("Channel has no producers defined".to_string());
        }

        // Validate diode_producers is subset of producers
        for diode in &self.diode_producers {
            if !self.producers.contains(diode) {
                errors.push(format!("Diode producer '{diode}' is not in producers list"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check if a producer is in diode mode (write-only).
    pub fn is_diode(&self, producer: &str) -> bool {
        self.diode_producers.iter().any(|d| d == producer)
    }

    /// Log configuration info for a channel.
    pub fn log_config_info(&self, channel_name: &str) {
        if !self.scanning.enable {
            info!("Channel '{channel_name}': scanning disabled (all files treated as clean)");
        }
        if self.scanning.permissive {
            info!(
                "Channel '{channel_name}': permissive mode enabled (scan errors treated as clean)"
            );
        }
        if !self.user_notify.enable {
            info!("Channel '{channel_name}': user notifications disabled");
        }
        if !self.diode_producers.is_empty() {
            info!(
                "Channel '{channel_name}': diode producers: {:?}",
                self.diode_producers
            );
        }
        if !self.scanning.ignore_file_patterns.is_empty() {
            debug!(
                "Channel '{channel_name}': ignoring file patterns: {:?}",
                self.scanning.ignore_file_patterns
            );
        }
        if !self.scanning.ignore_path_patterns.is_empty() {
            debug!(
                "Channel '{channel_name}': ignoring path patterns: {:?}",
                self.scanning.ignore_path_patterns
            );
        }
    }

    /// Load and validate configuration from file.
    pub fn load_config(config_path: &Path) -> Result<Config> {
        let config_data = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let mut config: Config =
            serde_json::from_str(&config_data).with_context(|| "Failed to parse config JSON")?;

        info!("Loaded configuration for {} channels", config.len());

        // Auto-add refresh trigger file to ignore patterns when guest notify is enabled
        for channel_config in config.values_mut() {
            if channel_config.guest_notify.is_some() {
                let pattern = REFRESH_TRIGGER_FILE.to_string();
                if !channel_config
                    .scanning
                    .ignore_file_patterns
                    .contains(&pattern)
                {
                    channel_config.scanning.ignore_file_patterns.push(pattern);
                }
            }
        }

        // Validate and filter channels
        let original_count = config.len();
        config.retain(
            |channel_name, channel_config| match channel_config.validate() {
                Ok(()) => {
                    channel_config.log_config_info(channel_name);
                    info!("Channel '{channel_name}': ready for operation");
                    true
                }
                Err(errors) => {
                    for err in &errors {
                        error!("Channel '{channel_name}': {err}");
                    }
                    warn!("Channel '{channel_name}': removed due to configuration errors");
                    false
                }
            },
        );

        // Validate base_path uniqueness across channels
        validate_unique_base_paths(&config)?;

        let final_count = config.len();
        if final_count < original_count {
            warn!(
                "Removed {} channels due to configuration issues",
                original_count - final_count
            );
        }

        if config.is_empty() {
            warn!("No valid channels remain after configuration validation");
            warn!("Daemon will start but perform no work until configuration is fixed");
        } else {
            info!("Starting daemon with {final_count} valid channels");
        }

        Ok(config)
    }
}

/// Validate that no two channels share the same `base_path`.
fn validate_unique_base_paths(config: &Config) -> Result<()> {
    let mut seen: HashMap<PathBuf, &str> = HashMap::new();

    for (name, channel) in config {
        let canonical = channel
            .base_path
            .canonicalize()
            .unwrap_or_else(|_| channel.base_path.clone());
        if let Some(existing) = seen.get(&canonical) {
            return Err(anyhow::anyhow!(
                "Channels '{}' and '{}' have conflicting base_path '{}'",
                existing,
                name,
                channel.base_path.display()
            ));
        }
        seen.insert(canonical, name);
    }

    Ok(())
}

/// Verify configuration file without starting daemon.
pub fn verify_config(config_path: &Path, verbose: bool) -> Result<()> {
    let config_data = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: Config =
        serde_json::from_str(&config_data).with_context(|| "Failed to parse config JSON")?;

    info!("Loaded configuration for {} channels", config.len());

    let mut total_valid = 0;
    let mut total_invalid = 0;

    for (channel_name, channel_config) in &config {
        if verbose {
            info!("Verifying channel '{channel_name}'...");
        }

        match channel_config.validate() {
            Ok(()) => {
                total_valid += 1;
                if verbose {
                    info!("Channel '{channel_name}': valid");
                }
            }
            Err(errors) => {
                total_invalid += 1;
                for err in &errors {
                    error!("Channel '{channel_name}': {err}");
                }
            }
        }
    }

    // Check base_path uniqueness
    validate_unique_base_paths(&config)?;

    info!(
        "Configuration verification complete: {total_valid} valid, {total_invalid} invalid channels"
    );

    if total_invalid > 0 {
        return Err(anyhow::anyhow!(
            "Configuration has {total_invalid} invalid channels"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_config() {
        let json = r#"{
            "test-channel": {
                "basePath": "/tmp/test",
                "producers": ["vm1", "vm2"],
                "consumers": ["vm3"],
                "scanning": {
                    "infectedAction": "quarantine",
                    "permissive": false,
                    "ignoreFilePatterns": [".crdownload", ".part", "~$"],
                    "ignorePathPatterns": [".Trash-"]
                }
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("test-channel").unwrap();

        assert_eq!(channel.base_path, PathBuf::from("/tmp/test"));
        assert_eq!(channel.producers, vec!["vm1", "vm2"]);
        assert_eq!(channel.consumers, vec!["vm3"]);
        assert_eq!(channel.scanning.infected_action, InfectedAction::Quarantine);
        assert!(!channel.scanning.permissive);
        assert_eq!(
            channel.scanning.ignore_file_patterns,
            vec![".crdownload", ".part", "~$"]
        );
        assert_eq!(channel.scanning.ignore_path_patterns, vec![".Trash-"]);
    }

    #[test]
    fn test_deserialize_minimal_config() {
        let json = r#"{
            "minimal": {
                "basePath": "/tmp/minimal",
                "producers": ["vm1"],
                "consumers": []
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("minimal").unwrap();

        assert!(channel.scanning.enable);
        assert_eq!(channel.scanning.infected_action, InfectedAction::Delete);
        assert!(!channel.scanning.permissive);
        assert!(channel.scanning.ignore_file_patterns.is_empty());
        assert!(channel.scanning.ignore_path_patterns.is_empty());
    }

    #[test]
    fn test_scanning_disabled() {
        let json = r#"{
            "no-scan": {
                "basePath": "/tmp/no-scan",
                "producers": ["trusted-vm"],
                "consumers": [],
                "scanning": {
                    "enable": false
                }
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("no-scan").unwrap();

        assert!(!channel.scanning.enable);
    }

    #[test]
    fn test_default_values() {
        let json = r#"{
            "defaults": {
                "basePath": "/tmp/defaults",
                "producers": ["vm1"],
                "consumers": []
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("defaults").unwrap();

        assert_eq!(channel.debounce_ms, 1000);
        assert!(channel.scanning.enable);
        assert!(channel.user_notify.enable);
        assert_eq!(
            channel.user_notify.socket,
            PathBuf::from("/run/clamav/notify.sock")
        );
        assert!(channel.guest_notify.is_none());
    }

    #[test]
    fn test_guest_notify_config() {
        let json = r#"{
            "with-notify": {
                "basePath": "/tmp/notify",
                "producers": ["vm1"],
                "consumers": [],
                "guestNotify": {
                    "guests": [3, 4, 5],
                    "port": 9999
                }
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("with-notify").unwrap();
        let guest_notify = channel.guest_notify.as_ref().unwrap();

        assert_eq!(guest_notify.guests, vec![3, 4, 5]);
        assert_eq!(guest_notify.port, 9999);
    }

    #[test]
    fn test_guest_notify_config_defaults() {
        let json = r#"{
            "notify-defaults": {
                "basePath": "/tmp/notify",
                "producers": ["vm1"],
                "consumers": [],
                "guestNotify": {
                    "guests": [3]
                }
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("notify-defaults").unwrap();
        let guest_notify = channel.guest_notify.as_ref().unwrap();

        assert_eq!(guest_notify.port, 3401);
    }

    #[test]
    fn test_user_notify_config() {
        let json = r#"{
            "with-user-notify": {
                "basePath": "/tmp/user-notify",
                "producers": ["vm1"],
                "consumers": [],
                "userNotify": {
                    "enable": false,
                    "socket": "/custom/notify.sock"
                }
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("with-user-notify").unwrap();

        assert!(!channel.user_notify.enable);
        assert_eq!(
            channel.user_notify.socket,
            PathBuf::from("/custom/notify.sock")
        );
    }

    #[test]
    fn test_unique_base_paths_ok() {
        let mut config = Config::new();
        config.insert(
            "ch1".to_string(),
            ChannelConfig {
                base_path: PathBuf::from("/tmp/ch1"),
                producers: vec!["vm1".to_string()],
                consumers: vec![],
                diode_producers: vec![],
                debounce_ms: 1000,
                scanning: ScanningConfig::default(),
                user_notify: UserNotifyConfig::default(),
                guest_notify: None,
            },
        );
        config.insert(
            "ch2".to_string(),
            ChannelConfig {
                base_path: PathBuf::from("/tmp/ch2"),
                producers: vec!["vm1".to_string()],
                consumers: vec![],
                diode_producers: vec![],
                debounce_ms: 1000,
                scanning: ScanningConfig::default(),
                user_notify: UserNotifyConfig::default(),
                guest_notify: None,
            },
        );

        assert!(validate_unique_base_paths(&config).is_ok());
    }

    #[test]
    fn test_unique_base_paths_conflict() {
        let mut config = Config::new();
        config.insert(
            "ch1".to_string(),
            ChannelConfig {
                base_path: PathBuf::from("/tmp/shared"),
                producers: vec!["vm1".to_string()],
                consumers: vec![],
                diode_producers: vec![],
                debounce_ms: 1000,
                scanning: ScanningConfig::default(),
                user_notify: UserNotifyConfig::default(),
                guest_notify: None,
            },
        );
        config.insert(
            "ch2".to_string(),
            ChannelConfig {
                base_path: PathBuf::from("/tmp/shared"),
                producers: vec!["vm2".to_string()],
                consumers: vec![],
                diode_producers: vec![],
                debounce_ms: 1000,
                scanning: ScanningConfig::default(),
                user_notify: UserNotifyConfig::default(),
                guest_notify: None,
            },
        );

        let result = validate_unique_base_paths(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("conflicting base_path")
        );
    }

    #[test]
    fn test_diode_config() {
        let json = r#"{
            "with-diode": {
                "basePath": "/tmp/diode",
                "producers": ["trusted-vm", "untrusted-vm"],
                "consumers": ["reader-vm"],
                "diodeProducers": ["untrusted-vm"]
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("with-diode").unwrap();

        assert_eq!(channel.diode_producers, vec!["untrusted-vm"]);
        assert!(channel.is_diode("untrusted-vm"));
        assert!(!channel.is_diode("trusted-vm"));
    }

    #[test]
    fn test_diode_default_empty() {
        let json = r#"{
            "no-diode": {
                "basePath": "/tmp/no-diode",
                "producers": ["vm1"],
                "consumers": []
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        let channel = config.get("no-diode").unwrap();

        assert!(channel.diode_producers.is_empty());
        assert!(!channel.is_diode("vm1"));
    }
}
