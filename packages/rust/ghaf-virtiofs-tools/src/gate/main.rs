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
use ghaf_virtiofs_tools::scanner::{ClamAVScanner, VirusScanner};
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
        #[arg(short, long)]
        debug: bool,
        /// Disable virus scanning (treat all files as clean)
        #[arg(long)]
        no_scan: bool,
    },
    /// Verify configuration file without starting daemon
    Verify {
        #[arg(short, long)]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            config,
            debug,
            no_scan,
        } => {
            init_logger(debug)?;

            let mut config = ChannelConfig::load_config(&config).with_context(|| {
                format!("Failed to load configuration from {}", config.display())
            })?;

            // Disable scanning for all channels if --no-scan is set
            if no_scan {
                log::info!("Virus scanning disabled via --no-scan flag");
                for channel in config.values_mut() {
                    channel.scanning.enable = false;
                }
            }

            let scanner: Arc<dyn VirusScanner + Send + Sync> = Arc::new(ClamAVScanner);
            if !no_scan {
                if let Err(e) = scanner.validate_availability() {
                    log::warn!(
                        "ClamAV unavailable: {e}. Only permissive channels will propagate files."
                    );
                }
            }

            let daemon = Daemon::new(config, scanner);
            daemon.run().await.context("Daemon execution failed")?;

            Ok(())
        }
        Commands::Verify { config } => verify_config(&config)
            .with_context(|| format!("Failed to verify configuration file {}", config.display())),
    }
}
