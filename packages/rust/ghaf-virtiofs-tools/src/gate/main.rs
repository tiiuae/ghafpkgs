// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Shared directories daemon for cross-VM file sharing with virus scanning.
//!
//! This daemon uses inotify to monitor inbound directories,
//! scans files with `ClamAV`, and promotes clean files to export.

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc)]

mod config;
mod daemon;
mod notify;

use config::{ChannelConfig, verify_config};
use daemon::Daemon;
use ghaf_virtiofs_tools::scanner::{ClamAVScanner, VirusScanner};
use ghaf_virtiofs_tools::util::init_logger;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "virtiofs-gate")]
#[command(about = "Virtiofs gateway daemon for cross-VM file sharing")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
enum Commands {
    /// Start the daemon
    Run {
        #[arg(short, long)]
        config: PathBuf,
        #[arg(short, long, default_value = "false")]
        debug: bool,
    },
    /// Verify configuration file without starting daemon
    Verify {
        #[arg(short, long)]
        config: PathBuf,
        #[arg(short, long, default_value = "false")]
        verbose: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { config, debug } => {
            init_logger(debug)?;

            let config = ChannelConfig::load_config(&config).with_context(|| {
                format!("Failed to load configuration from {}", config.display())
            })?;

            let scanner = Arc::new(ClamAVScanner);
            if let Err(e) = scanner.validate_availability() {
                log::warn!(
                    "ClamAV unavailable: {e}. Only permissive channels will propagate files."
                );
            }

            let daemon = Daemon::new(config, scanner);
            daemon
                .run()
                .await
                .with_context(|| "Daemon execution failed")?;

            Ok(())
        }
        Commands::Verify { config, verbose } => verify_config(&config, verbose)
            .with_context(|| format!("Failed to verify configuration file {}", config.display())),
    }
}
