/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
//! Client for the device manager API (vhotplug or ghaf-device-manager):
//! newline-delimited JSON over vsock.
use std::io;
use std::time::Duration;

use cosmic::iced::futures::Stream;
use cosmic::iced::futures::channel::mpsc;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_vsock::{VsockAddr, VsockStream};

pub const HOST_CID: u32 = 2;
pub const DEFAULT_PORT: u32 = 2000;

const RECONNECT_DELAY: Duration = Duration::from_secs(3);

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Response {
    result: Option<String>,
    error: Option<String>,
    pub pci_devices: Option<Vec<Listed>>,
    pub usb_devices: Option<Vec<Listed>>,
}

/// A device in a `pci_list` or `usb_list` reply.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Listed {
    /// VM the device is attached to, if any.
    pub vm: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Event {
    event: String,
}

async fn connect(port: u32) -> io::Result<BufReader<VsockStream>> {
    let stream = VsockStream::connect(VsockAddr::new(HOST_CID, port)).await?;
    Ok(BufReader::new(stream))
}

async fn read_line(stream: &mut BufReader<VsockStream>) -> io::Result<String> {
    let mut line = String::new();
    if stream.read_line(&mut line).await? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed",
        ));
    }
    Ok(line)
}

/// Send one message and return its reply, or the error the reply carries.
async fn request(stream: &mut BufReader<VsockStream>, msg: &Value) -> Result<Response, String> {
    let exchange = async {
        stream.write_all(format!("{msg}\n").as_bytes()).await?;
        let line = read_line(stream).await?;
        serde_json::from_str::<Response>(&line)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    };
    let resp = exchange.await.map_err(|e| e.to_string())?;
    if resp.result.as_deref() != Some("ok") {
        return Err(resp.error.unwrap_or_else(|| "unknown error".into()));
    }
    Ok(resp)
}

/// Send `msg` on a new connection and return its reply.
pub async fn call(port: u32, msg: Value) -> Result<Response, String> {
    let mut stream = connect(port)
        .await
        .map_err(|e| format!("cannot reach the device manager on vsock port {port}: {e}"))?;
    request(&mut stream, &msg).await
}

async fn listen(port: u32, output: &mut mpsc::Sender<String>) -> Result<(), String> {
    let mut stream = connect(port).await.map_err(|e| e.to_string())?;
    request(&mut stream, &json!({"action": "enable_notifications"})).await?;
    tracing::info!("Listening for device manager notifications on port {port}");
    loop {
        let line = read_line(&mut stream).await.map_err(|e| e.to_string())?;
        match serde_json::from_str::<Event>(&line) {
            Ok(event) => {
                if let Err(e) = output.try_send(event.event) {
                    tracing::warn!("Dropping notification: {e}");
                }
            }
            Err(e) => tracing::error!("Invalid JSON in notification ({e}): {}", line.trim()),
        }
    }
}

/// Endless stream of notification event names, such as `pci_attached`;
/// reconnects after failures.
pub fn events(port: u32) -> impl Stream<Item = String> {
    cosmic::iced::stream::channel(16, async move |mut output: mpsc::Sender<String>| {
        loop {
            if let Err(e) = listen(port, &mut output).await {
                tracing::warn!(
                    "Notification listener error: {e}; reconnecting in {}s",
                    RECONNECT_DELAY.as_secs()
                );
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}
