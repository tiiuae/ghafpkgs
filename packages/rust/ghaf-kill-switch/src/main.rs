/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
//! Command line interface to the kill switch. It replaces the `ghaf-killswitch`
//! script and keeps its commands: `status` prints `<device>: blocked|unblocked`
//! lines.
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ghaf_kill_switch::DEFAULT_PORT;
use ghaf_kill_switch::killswitch::{self, Device};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ghaf-kill-switch",
    about = "Tool for blocking and unblocking devices"
)]
struct Args {
    /// Device manager (vhotplug) vsock port
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u32,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Block a device, or all of them
    Block(Target),
    /// Unblock a device, or all of them
    Unblock(Target),
    /// List the devices present on this system
    List,
    /// Show whether each present device is blocked
    Status,
}

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Target {
    /// Device to change: mic, cam, net or bluetooth
    device: Option<String>,
    /// Change every present device
    #[arg(long)]
    all: bool,
}

async fn set(port: u32, target: Target, enabled: bool) -> Result<(), String> {
    let Some(arg) = target.device else {
        return killswitch::set_all(port, enabled).await;
    };
    let device = Device::from_arg(&arg).ok_or_else(|| format!("{arg} is not supported"))?;
    if killswitch::status(port).await?.get(device).is_none() {
        return Err(format!("{arg} is not present on this system"));
    }
    killswitch::set(port, device, enabled).await
}

async fn run(args: Args) -> Result<(), String> {
    match args.command {
        Command::Block(target) => set(args.port, target, false).await,
        Command::Unblock(target) => set(args.port, target, true).await,
        Command::List => {
            for (device, _) in killswitch::status(args.port).await?.present() {
                println!("{}", device.arg());
            }
            Ok(())
        }
        Command::Status => {
            for (device, enabled) in killswitch::status(args.port).await?.present() {
                let state = if enabled { "unblocked" } else { "blocked" };
                println!("{}: {state}", device.arg());
            }
            Ok(())
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("warn"))
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
