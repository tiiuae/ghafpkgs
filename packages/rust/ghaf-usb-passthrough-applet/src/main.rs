/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(about = "USB passthrough panel applet for Ghaf")]
struct Args {
    /// vhotplug server vsock port
    #[arg(long, default_value_t = ghaf_usb_passthrough_applet::DEFAULT_PORT)]
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
            "warn,ghaf_usb_passthrough_applet={}",
            args.loglevel
        )))
        // The journal adds its own timestamps and does not render colors.
        .without_time()
        .with_ansi(false)
        .init();

    tracing::info!("Starting USB passthrough applet with version {VERSION}");

    ghaf_usb_passthrough_applet::run(args.port)
}
