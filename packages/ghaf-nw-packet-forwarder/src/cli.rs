/*
    Copyright 2025 TII (SSRC) and the contributors
    SPDX-License-Identifier: Apache-2.0
*/
use clap::Parser;
use clap::ValueEnum;
use lazy_static::lazy_static;
use pnet::ipnetwork::IpNetwork;
use pnet::util::MacAddr;
use std::error::Error;
use std::str;

lazy_static! {
    static ref CLI_ARGS: Args = {


        // Initialize the IP address using a function or any other logic
        let args=handling_args().expect("Error in argument handling");
        println!("{args:?}");
        args
    };
}

#[derive(ValueEnum, Default, Debug, Clone, Copy, PartialEq)]
pub enum LogOutput {
    #[default]
    Syslog,
    Stdout,
}

/// Network Packet forwarder tool for Ghaf
#[derive(Parser, Debug)]
#[command(author = "Enes Öztürk")]
#[command(name = "Network Packet Forwarder")]
#[command(about = "Packet forwarder between two network interfaces for Ghaf.")]
#[command(long_about =None /* ,version =VERSION*/)]
struct Args {
    /// Name of the external network interface
    #[arg(long)]
    external_iface: String,

    /// Name of the internal network interface
    #[arg(long)]
    internal_iface: String,

    /// IP address of the external network interface
    #[arg(long)]
    external_ip: Option<IpNetwork>,

    /// IP address of the internal network interface
    #[arg(long)]
    internal_ip: Option<IpNetwork>,

    /// Enable Chromecast packet forwarding functionality
    #[arg(long, default_value_t = 0)]
    chromecast: u8,

    /// Chrome VM Ip address
    #[arg(long)]
    chromevm_ip: Option<IpNetwork>,

    /// Chrome VM Mac address   
    #[arg(long)]
    chromevm_mac: Option<MacAddr>,

    /// Log severity
    #[arg(long, default_value_t = log::Level::Info)]
    pub log_level: log::Level,

    /// Log output
    #[arg(long, value_enum, default_value_t = Default::default())]
    pub log_output: LogOutput,
}

fn handling_args() -> Result<Args, Box<dyn Error>> {
    let args: Args = Args::parse();
    args.validate();
    Ok(args)
}

impl Args {
    fn validate(&self) {
        if 1 == self.chromecast && (self.chromevm_ip.is_none() || self.chromevm_mac.is_none()) {
            panic!("Error: --chromevm_ip and --chromevm_mac are required when --chromecast is enabled.");
        }
    }
}

pub fn get_ext_iface_name() -> &'static str {
    CLI_ARGS.external_iface.as_str()
}
pub fn get_int_iface_name() -> &'static str {
    CLI_ARGS.internal_iface.as_str()
}

pub fn get_ext_ip() -> Option<IpNetwork> {
    CLI_ARGS.external_ip
}

pub fn get_int_ip() -> Option<IpNetwork> {
    CLI_ARGS.internal_ip
}

pub fn get_chromecast() -> bool {
    CLI_ARGS.chromecast == 1
}

pub fn get_chromevm_ip() -> IpNetwork {
    CLI_ARGS.chromevm_ip.unwrap()
}

pub fn get_chromevm_mac() -> MacAddr {
    CLI_ARGS.chromevm_mac.unwrap()
}

pub fn get_log_level() -> &'static log::Level {
    &CLI_ARGS.log_level
}

pub fn get_log_output() -> &'static LogOutput {
    &CLI_ARGS.log_output
}
