/*
    SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: Apache-2.0
*/
mod cli;
mod filter;
mod forward_impl; // Declare the forward module

use cli::LogOutput;
use env_logger::Builder;
use filter::Chromecast;
use filter::chromecast::{ExternalOps, InternalOps};
use forward_impl::forward::{self, get_ifaces};
use log::{debug, error, info, trace, warn};
use pnet::datalink::DataLinkReceiver;
use pnet::datalink::{self, Channel::Ethernet, Config};
use pnet::packet::ethernet::MutableEthernetPacket;
use std::panic;
use std::sync::Arc;
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::signal;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    initialize_logger();

    // Get the network interfaces inside the async block to ensure it lives long enough
    let interfaces = datalink::interfaces();

    // Find the external interface
    let external_iface = interfaces
        .iter()
        .find(|iface| iface.name == cli::get_ext_iface_name() && !iface.is_loopback())
        .expect("No matching external interface found")
        .clone(); // Clone the interface to avoid borrowing issues

    // Find the internal interface
    let internal_iface: datalink::NetworkInterface = interfaces
        .iter()
        .find(|iface| iface.name == cli::get_int_iface_name() && !iface.is_loopback())
        .expect("No matching internal interface found")
        .clone(); // Clone the interface to avoid borrowing issues
    info!(
        "Using interfaces: {},ip:{:?} and {}, ip:{:?}",
        external_iface.name, external_iface.ips, internal_iface.name, internal_iface.ips
    );

    // Assign interfaces
    if let Err(e) = forward::assign_ifaces(
        &external_iface,
        &internal_iface,
        cli::get_ext_ip(),
        cli::get_int_ip(),
    ) {
        error!("Failed to assign interfaces: {e}");
        std::process::exit(1); // Optional: Exit with a specific non-zero code
    }

    debug!("ifaces:{:?}", forward::get_ifaces());

    // Create channels for both interfaces
    let config = Config::default();
    let (internal_tx_ch, internal_rx_ch) = match datalink::channel(&internal_iface, config) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!(
            "Failed to create datalink channel for {}: {}",
            internal_iface.name, e
        ),
    };

    let (external_tx_ch, external_rx_ch) = match datalink::channel(&external_iface, config) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!(
            "Failed to create datalink channel for {}: {}",
            external_iface.name, e
        ),
    };

    // Wrap `external tx,rx` and `internal tx,rx` in Arc<Mutex<>> for thread-safe access
    let external_tx_ch = Arc::new(Mutex::new(external_tx_ch));
    let external_rx_ch = Arc::new(Mutex::new(external_rx_ch));
    let internal_tx_ch = Arc::new(Mutex::new(internal_tx_ch));
    let internal_rx_ch = Arc::new(Mutex::new(internal_rx_ch));

    // Create a CancellationToken
    let token = CancellationToken::new();

    // Security algorithms init
    forward::set_sec_params(&cli::get_ratelimiting_ops(), token.clone()).await;

    // chromecast feature enabling
    let chromecast = Arc::new(Mutex::new(Chromecast::new(forward::get_ifaces())));
    // Lock only once here for external_ops
    let chromecast_external = chromecast.lock().await.get_external_ops();
    // Lock only once here for internal_ops
    let chromecast_internal = chromecast.lock().await.get_internal_ops();

    // Spawn an async thread for packet processing (capture loop) on internal interface
    let internal_task = tokio::task::spawn({
        let cancel_token = token.clone();
        let internal_iface = internal_iface.clone();
        let ifaces = get_ifaces();
        let mut last_err = String::new();

        async move {
            info!("Starting packet capture on {}...", internal_iface.name);
            let internal_rx_ch = Arc::clone(&internal_rx_ch); // Clone for the async block

            loop {
                tokio::select! {
                    // Check the cancellation token
                    () = cancel_token.cancelled() => {
                        // Token was cancelled, clean up and exit task
                        warn!("Cancellation token triggered, shutting down capture on {}...", internal_iface.name);
                        break;
                    }
                    () = async {
                        if forward::is_iface_running_up(&internal_iface.name) {
                            match capture_next_packet(&internal_rx_ch).await {
                                Ok(mut frame) => {
                                    process_internal_packets(&chromecast_internal, &external_tx_ch, &mut frame, &internal_iface, &ifaces).await;
                                }
                                Err(e) => {
                                    if last_err != e {
                                        error!("Error receiving packet on {}: {}", internal_iface.name, e);
                                        last_err = e;
                                    }
                                }
                            }
                        } else {
                            // Sleep for 1 second asynchronously
                            sleep(Duration::new(1, 0)).await;
                        }
                    } => {}
                }
            }

            warn!("Task for {} is cleaning up", internal_iface.name);
        }
    });

    // Spawn a blocking thread for packet processing (capture loop) on external interface
    let external_task = tokio::task::spawn({
        let internal_iface = internal_iface.clone();
        let cancel_token = token.clone();
        let mut last_err = String::new();
        async move {
            info!("Starting packet capture on {}...", external_iface.name);
            let chromecast_external = chromecast_external.clone(); // Clone Arc to give external task access

            loop {
                tokio::select! {
                    // Check the cancellation token
                    () = cancel_token.cancelled() => {
                        // Token was cancelled, clean up and exit task
                        warn!("Cancellation token triggered, shutting down capture on {}...", external_iface.name);
                        break;
                    }
                    () = async {
                        if forward::is_iface_running_up(&external_iface.name) {
                            match capture_next_packet(&external_rx_ch).await {
                                Ok(mut frame) => {
                                    process_external_packets(&chromecast_external, &internal_tx_ch, &mut frame, &external_iface, &internal_iface).await;
                                }
                                Err(e) => {
                                    if last_err != e {
                                        error!("Error receiving packet on {}: {}", external_iface.name, e);
                                        last_err = e;
                                    }
                                }
                            }
                        } else {
                            // Sleep for 1 second asynchronously
                            sleep(Duration::new(1, 0)).await;
                        }
                    } => {}
                }
            }

            warn!("Task for {} is cleaning up", external_iface.name);
        }
    });

    // Gracefully handle shutdown (e.g., on SIGINT)
    let shutdown = signal::ctrl_c().await;
    if let Err(e) = shutdown {
        error!("Error while waiting for shutdown signal: {e}");
    }
    info!("Shutting down gracefully...");
    // Send a cancellation signal
    token.cancel();

    // Wait for the tasks to finish
    let _ = tokio::join!(external_task, internal_task);
}

/// Initializes the logging system based on the selected feature and runtime configuration.
///
/// - If `tokio-console` is enabled, initializes the `console_subscriber`.
/// - Otherwise, configures either `stdout` logging or `syslog` based on user input.
///   Panics if an invalid log output is specified.
fn initialize_logger() {
    #[cfg(feature = "tokio-console")]
    {
        console_subscriber::init();
    }
    #[cfg(not(feature = "tokio-console"))]
    {
        // Initialize env_logger
        let log_output = cli::get_log_output();
        let log_level = cli::get_log_level().to_level_filter();
        if LogOutput::Stdout == *log_output {
            // You can set the level in code here
            Builder::new()
                .filter_level(log_level) // Set to Debug level in code
                .init();
            print!("Logging to stdout");
        } else if LogOutput::Syslog == *log_output {
            print!("Logging to syslog");
            let formatter = Formatter3164 {
                facility: Facility::LOG_USER,
                hostname: None,
                process: "nw-packet-forwarder".into(),
                pid: 0,
            };
            let logger = match syslog::unix(formatter) {
                Err(e) => {
                    println!("impossible to connect to syslog: {e:?}");
                    return;
                }
                Ok(logger) => logger,
            };
            log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
                .map(|()| log::set_max_level(log_level))
                .expect("Failed to set logger");
        } else {
            panic!("Invalid log output");
        }

        debug!("Logger initialized");
    }
}

async fn capture_next_packet(
    rx_channel: &Arc<tokio::sync::Mutex<Box<dyn DataLinkReceiver>>>,
) -> Result<Vec<u8>, String> {
    let join_handle = tokio::task::spawn_blocking({
        let rx_channel = Arc::clone(rx_channel);
        move || {
            let mut rx = rx_channel.blocking_lock();
            rx.next().map(|frame| frame.to_owned())
        }
    })
    .await
    .map_err(|e| format!("Error in spawn_blocking: {e}"));

    match join_handle {
        Ok(Ok(frame)) => Ok(frame),
        Ok(Err(e)) => Err(format!("Error receiving packet: {e}")),
        Err(e) => Err(e.to_string()),
    }
}

async fn process_internal_packets(
    chromecast_internal: &Arc<InternalOps>,
    external_tx_ch: &Arc<Mutex<Box<dyn pnet::datalink::DataLinkSender>>>,
    frame: &mut [u8],
    internal_iface: &datalink::NetworkInterface,
    ifaces: &forward::Ifaces,
) {
    if let Some(mut eth_packet) = MutableEthernetPacket::new(frame) {
        if chromecast_internal
            .int_to_ext_filter_packets(&eth_packet.to_immutable())
            .await
        {
            forward::internal_to_external_process_packet(external_tx_ch, &mut eth_packet, ifaces)
                .await;

            trace!(
                "Received frame on {}: {}",
                internal_iface.name,
                forward::parse_packet(&eth_packet)
            );
        }
    } else {
        warn!(
            "Invalid Ethernet packet received on {}",
            internal_iface.name
        );
    }
}

async fn process_external_packets(
    chromecast_external: &Arc<ExternalOps>,
    internal_tx_ch: &Arc<Mutex<Box<dyn pnet::datalink::DataLinkSender>>>,
    frame: &mut [u8],
    external_iface: &datalink::NetworkInterface,
    internal_iface: &datalink::NetworkInterface,
) {
    // Forward packet to internal interface channel
    let internal_tx_ch_clone = Arc::clone(internal_tx_ch);

    if let Some(mut eth_packet) = MutableEthernetPacket::new(frame) {
        if let Some((mac, ip)) = chromecast_external
            .is_ext_to_int_packet(&eth_packet.to_immutable())
            .await
        {
            forward::external_to_internal_process_packet(
                internal_tx_ch_clone,
                &mut eth_packet,
                &external_iface.ips,
                internal_iface.mac.unwrap(),
                mac,
                ip,
            )
            .await;
        }
        trace!(
            "Received frame on {}: {}",
            external_iface.name,
            forward::parse_packet(&eth_packet)
        );
    }
}
