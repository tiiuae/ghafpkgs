// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

mod config;
mod daemon;
mod notify;
mod sync;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;

use config::{ChannelConfig, verify_config};
use daemon::Daemon;
use ghaf_virtiofs_tools::scanner::{ClamAVScanner, NoopScanner, VirusScanner};
use ghaf_virtiofs_tools::util::init_logger;

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
        /// Disable virus scanning (treat all files as clean)
        #[arg(long, default_value = "false")]
        no_scan: bool,
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
        Commands::Run { config, debug, no_scan } => {
            init_logger(debug)?;

            let config = ChannelConfig::load_config(&config).with_context(|| {
                format!("Failed to load configuration from {}", config.display())
            })?;

            let scanner: Arc<dyn VirusScanner + Send + Sync> = if no_scan {
                Arc::new(NoopScanner)
            } else {
                let scanner = Arc::new(ClamAVScanner);
                if let Err(e) = scanner.validate_availability() {
                    log::warn!("ClamAV unavailable: {e}. Only permissive channels will propagate files.");
                }
                scanner
            };

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
