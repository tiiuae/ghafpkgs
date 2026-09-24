/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(about = "Privacy kill switch panel applet for Ghaf")]
struct Args {
    /// Device manager (vhotplug) vsock port
    #[arg(long, default_value_t = ghaf_kill_switch::DEFAULT_PORT)]
    port: u32,
    /// Log level of this applet: off, error, warn, info, debug, trace.
    /// Libraries (libcosmic, iced, ...) always log at warn and above.
    #[arg(long, default_value = "info")]
    loglevel: LevelFilter,
}

fn main() -> cosmic::iced::Result {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(format!(
            "warn,ghaf_kill_switch={0},{1}={0}",
            args.loglevel,
            env!("CARGO_CRATE_NAME")
        )))
        // The journal adds its own timestamps and does not render colors.
        .without_time()
        .with_ansi(false)
        .init();

    tracing::info!("Starting kill switch applet with version {VERSION}");

    ghaf_kill_switch::run(args.port)
}
