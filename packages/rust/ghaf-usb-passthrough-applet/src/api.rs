/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
//! Client for the vhotplug USB passthrough API (newline-delimited JSON over vsock).

use std::io;
use std::time::Duration;

use cosmic::iced::futures::Stream;
use cosmic::iced::futures::channel::mpsc;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio_vsock::{VsockAddr, VsockStream};

pub const HOST_CID: u32 = 2;
pub const DEFAULT_PORT: u32 = 2000;

const UNKNOWN_DEVICE: &str = "<unknown device>";
const NOTIFICATION_NAME_LEN: usize = 20;
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const LIST_ATTEMPTS: u32 = 5;
const LIST_RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Device {
    pub device_node: Option<String>,
    pub product_name: Option<String>,
    pub vm: Option<String>,
    pub allowed_vms: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Response {
    pub result: Option<String>,
    pub event: Option<String>,
    pub error: Option<String>,
    pub usb_devices: Option<Vec<Device>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Event {
    pub event: String,
    pub usb_device: Option<Device>,
    pub allowed_vms: Option<Vec<String>>,
}

/// A device as shown in the popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub device_node: String,
    pub vm: String,
    pub options: Vec<String>,
}

/// Display name for a device: missing or all-digit names become `<unknown device>`,
/// underscores become spaces.
pub fn format_product_name(name: Option<&str>) -> String {
    match name {
        None => UNKNOWN_DEVICE.into(),
        Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => UNKNOWN_DEVICE.into(),
        Some(n) => n.replace('_', " "),
    }
}

/// Converts a `usb_list` reply into popup rows.
///
/// Devices without a node, name or allowed VMs are skipped; every device can
/// be detached, so a "None" option is always offered.
pub fn build_entries(devices: Vec<Device>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut dup_idx = 1;
    for dev in devices {
        let (Some(device_node), Some(raw_name), Some(mut options)) =
            (dev.device_node, dev.product_name, dev.allowed_vms)
        else {
            continue;
        };
        if options.is_empty() {
            continue;
        }
        if !options.iter().any(|o| o == "None" || o == "none") {
            options.push("None".into());
        }
        let mut name = format_product_name(Some(&raw_name));
        if entries.iter().any(|e| e.name == name) {
            name = format!("{name}({dup_idx})");
            dup_idx += 1;
        }
        entries.push(Entry {
            name,
            device_node,
            vm: dev.vm.unwrap_or_else(|| "None".into()),
            options,
        });
    }
    entries
}

pub fn attach_outcome(resp: &Response) -> Result<(), String> {
    if resp.result.as_deref() == Some("ok") || resp.event.as_deref() == Some("usb_attached") {
        Ok(())
    } else {
        Err(resp.error.clone().unwrap_or_else(|| "Unknown error".into()))
    }
}

/// For a `usb_select_vm` event with at least one VM to choose from, returns
/// the device node and a short display name.
pub fn pending_device(ev: &Event) -> Option<(String, String)> {
    if ev.event != "usb_select_vm" || ev.allowed_vms.as_ref().is_none_or(Vec::is_empty) {
        return None;
    }
    let dev = ev.usb_device.as_ref()?;
    let name = format_product_name(dev.product_name.as_deref())
        .chars()
        .take(NOTIFICATION_NAME_LEN)
        .collect();
    Some((dev.device_node.clone()?, name))
}

async fn connect(port: u32) -> io::Result<BufReader<VsockStream>> {
    let stream = VsockStream::connect(VsockAddr::new(HOST_CID, port)).await?;
    Ok(BufReader::new(stream))
}

async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<String>> {
    let mut line = String::new();
    Ok((reader.read_line(&mut line).await? > 0).then_some(line))
}

/// Sends one JSON message and reads one JSON reply line.
async fn request<S>(stream: &mut BufReader<S>, msg: &Value) -> io::Result<Response>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(format!("{msg}\n").as_bytes()).await?;
    let line = read_line(stream)
        .await?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed"))?;
    serde_json::from_str(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

async fn call(port: u32, msg: &Value) -> io::Result<Response> {
    request(&mut connect(port).await?, msg).await
}

pub async fn list_devices(port: u32) -> Result<Vec<Entry>, String> {
    let resp = call(port, &json!({"action": "usb_list"}))
        .await
        .map_err(|e| format!("Device fetch failed: {e}"))?;
    if resp.result.as_deref() != Some("ok") {
        let error = resp.error.unwrap_or_else(|| "unknown error".into());
        return Err(format!("Device fetch failed: {error}"));
    }
    Ok(build_entries(resp.usb_devices.unwrap_or_default()))
}

pub async fn list_devices_retry(port: u32) -> Result<Vec<Entry>, String> {
    let mut attempt = 1;
    loop {
        match list_devices(port).await {
            Err(e) if attempt < LIST_ATTEMPTS => {
                tracing::error!("{e}, trying again ({attempt})");
                attempt += 1;
                tokio::time::sleep(LIST_RETRY_DELAY).await;
            }
            result => return result,
        }
    }
}

/// Passes `device_node` through to `vm`, or detaches it when `vm` is "None".
pub async fn attach(port: u32, device_node: String, vm: String) -> Result<(), String> {
    let detach = vm.eq_ignore_ascii_case("none");
    let msg = if detach {
        json!({"action": "usb_detach", "device_node": device_node})
    } else {
        json!({"action": "usb_attach", "device_node": device_node, "vm": vm})
    };
    let resp = call(port, &msg).await.map_err(|e| e.to_string())?;
    if detach {
        Ok(())
    } else {
        attach_outcome(&resp)
    }
}

/// Calls `on_event` for every JSON line until the connection closes, which
/// is reported as an error so the caller reconnects.
async fn read_events<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    mut on_event: impl FnMut(Event),
) -> io::Result<()> {
    while let Some(line) = read_line(reader).await? {
        match serde_json::from_str::<Event>(&line) {
            Ok(event) => on_event(event),
            Err(e) => tracing::error!("Invalid JSON in notification ({e}): {}", line.trim()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "notification connection closed by remote",
    ))
}

async fn listen(port: u32, output: &mut mpsc::Sender<Event>) -> io::Result<()> {
    let mut stream = connect(port).await?;
    let resp = request(&mut stream, &json!({"action": "enable_notifications"})).await?;
    if resp.result.as_deref() != Some("ok") {
        tracing::error!("Failed to enable notifications: {resp:?}");
    }
    tracing::info!("Listening for vhotplug notifications on port {port}");
    read_events(&mut stream, |event| {
        if let Err(e) = output.try_send(event) {
            tracing::warn!("Dropping notification: {e}");
        }
    })
    .await
}

/// Endless stream of vhotplug notifications; reconnects after failures.
pub fn events(port: &u32) -> impl Stream<Item = Event> + use<> {
    let port = *port;
    cosmic::iced::stream::channel(16, async move |mut output: mpsc::Sender<Event>| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncWriteExt, BufReader, duplex};

    fn device(
        node: &str,
        name: Option<&str>,
        vm: Option<&str>,
        allowed: Option<&[&str]>,
    ) -> Device {
        Device {
            device_node: Some(node.into()),
            product_name: name.map(Into::into),
            vm: vm.map(Into::into),
            allowed_vms: allowed.map(|a| a.iter().map(|s| (*s).to_string()).collect()),
        }
    }

    #[test]
    fn product_name_formatting() {
        assert_eq!(format_product_name(None), "<unknown device>");
        assert_eq!(format_product_name(Some("1234")), "<unknown device>");
        assert_eq!(
            format_product_name(Some("My_Flash_Drive")),
            "My Flash Drive"
        );
    }

    #[test]
    fn entries_skip_unusable_devices() {
        let devices = vec![
            device("/dev/bus/usb/001/001", Some("NoVms"), None, None),
            device("/dev/bus/usb/001/002", Some("EmptyVms"), None, Some(&[])),
            device("/dev/bus/usb/001/003", None, None, Some(&["gui-vm"])),
            Device {
                device_node: None,
                ..device("", Some("NoNode"), None, Some(&["gui-vm"]))
            },
        ];
        assert!(build_entries(devices).is_empty());
    }

    #[test]
    fn entries_add_none_option_and_default_vm() {
        let entries = build_entries(vec![device(
            "/dev/bus/usb/001/004",
            Some("Flash_Drive"),
            None,
            Some(&["gui-vm", "chrome-vm"]),
        )]);
        assert_eq!(
            entries,
            vec![Entry {
                name: "Flash Drive".into(),
                device_node: "/dev/bus/usb/001/004".into(),
                vm: "None".into(),
                options: vec!["gui-vm".into(), "chrome-vm".into(), "None".into()],
            }]
        );
    }

    #[test]
    fn entries_keep_existing_none_and_current_vm() {
        let entries = build_entries(vec![device(
            "/dev/bus/usb/001/005",
            Some("Cam"),
            Some("chrome-vm"),
            Some(&["chrome-vm", "none"]),
        )]);
        assert_eq!(entries[0].options, vec!["chrome-vm", "none"]);
        assert_eq!(entries[0].vm, "chrome-vm");
    }

    #[test]
    fn entries_disambiguate_duplicate_names() {
        let allowed: &[&str] = &["gui-vm"];
        let entries = build_entries(vec![
            device("/a", Some("Disk"), None, Some(allowed)),
            device("/b", Some("Disk"), None, Some(allowed)),
            device("/c", Some("Disk"), None, Some(allowed)),
        ]);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Disk", "Disk(1)", "Disk(2)"]);
    }

    #[test]
    fn attach_outcome_interprets_response() {
        let ok = Response {
            result: Some("ok".into()),
            ..Default::default()
        };
        assert_eq!(attach_outcome(&ok), Ok(()));

        let attached = Response {
            event: Some("usb_attached".into()),
            ..Default::default()
        };
        assert_eq!(attach_outcome(&attached), Ok(()));

        let failed = Response {
            result: Some("failed".into()),
            error: Some("VM not running".into()),
            ..Default::default()
        };
        assert_eq!(attach_outcome(&failed), Err("VM not running".into()));

        assert_eq!(
            attach_outcome(&Response::default()),
            Err("Unknown error".into())
        );
    }

    #[test]
    fn pending_device_only_for_select_vm_with_choices() {
        let ev = Event {
            event: "usb_select_vm".into(),
            usb_device: Some(device(
                "/dev/bus/usb/002/003",
                Some("A_Very_Long_Product_Name_Here"),
                None,
                None,
            )),
            allowed_vms: Some(vec!["gui-vm".into()]),
        };
        assert_eq!(
            pending_device(&ev),
            Some(("/dev/bus/usb/002/003".into(), "A Very Long Product ".into()))
        );

        let no_choices = Event {
            allowed_vms: Some(vec![]),
            ..ev.clone()
        };
        assert_eq!(pending_device(&no_choices), None);

        let other = Event {
            event: "usb_connected".into(),
            ..ev
        };
        assert_eq!(pending_device(&other), None);
    }

    #[test]
    fn event_parses_from_json() {
        let ev: Event = serde_json::from_str(
            r#"{"event":"usb_select_vm","usb_device":{"device_node":"/x","product_name":"P","vid":"1234"},"allowed_vms":["gui-vm"]}"#,
        )
        .unwrap();
        assert_eq!(ev.event, "usb_select_vm");
        assert_eq!(ev.usb_device.unwrap().device_node.as_deref(), Some("/x"));
    }

    #[tokio::test]
    async fn request_sends_one_line_and_parses_reply() {
        let (client, server) = duplex(1024);
        let server = tokio::spawn(async move {
            let mut server = BufReader::new(server);
            let line = read_line(&mut server).await.unwrap().unwrap();
            server
                .write_all(b"{\"result\":\"ok\",\"usb_devices\":[]}\n")
                .await
                .unwrap();
            line
        });

        let resp = request(&mut BufReader::new(client), &json!({"action": "usb_list"}))
            .await
            .unwrap();

        assert_eq!(resp.result.as_deref(), Some("ok"));
        assert_eq!(server.await.unwrap(), "{\"action\":\"usb_list\"}\n");
    }

    #[tokio::test]
    async fn request_fails_when_connection_closes_without_reply() {
        let (client, server) = duplex(1024);
        let server = tokio::spawn(async move {
            let mut server = BufReader::new(server);
            read_line(&mut server).await.unwrap();
            // dropped here: no reply
        });

        let result = request(&mut BufReader::new(client), &json!({"action": "usb_list"})).await;
        server.await.unwrap();

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_events_yields_valid_lines_until_eof() {
        let (mut tx, rx) = duplex(1024);
        tx.write_all(b"{\"event\":\"usb_connected\"}\nnot json\n{\"event\":\"usb_select_vm\"}\n")
            .await
            .unwrap();
        drop(tx);

        let mut got = Vec::new();
        let result = read_events(&mut BufReader::new(rx), |e| got.push(e.event)).await;

        assert!(
            result.is_err(),
            "EOF must be reported so the caller reconnects"
        );
        assert_eq!(got, ["usb_connected", "usb_select_vm"]);
    }
}
