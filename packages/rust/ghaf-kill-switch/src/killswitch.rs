/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
//! Blocks devices by detaching them from their VMs through the device manager.
use std::future;

use cosmic::iced::futures::future::join_all;
use cosmic::iced::futures::{Stream, StreamExt};
use serde_json::json;

use crate::api;

/// A device the kill switch can block. Each maps to the passthrough rules
/// tagged `tag()` on `bus()`; which of them exist depends on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Microphone,
    Camera,
    WiFi,
    Bluetooth,
}

impl Device {
    pub const ALL: [Self; 4] = [Self::Microphone, Self::Camera, Self::WiFi, Self::Bluetooth];

    /// Name of the device on the `ghaf-kill-switch` command line.
    pub fn arg(self) -> &'static str {
        match self {
            Self::Microphone => "mic",
            Self::Camera => "cam",
            Self::WiFi => "net",
            Self::Bluetooth => "bluetooth",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Microphone => "Microphone",
            Self::Camera => "Camera",
            Self::WiFi => "Wi-Fi",
            Self::Bluetooth => "Bluetooth",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Microphone => "microphone-sensitivity-medium-symbolic",
            Self::Camera => "camera-photo-symbolic",
            Self::WiFi => "network-wireless-symbolic",
            Self::Bluetooth => "bluetooth-symbolic",
        }
    }

    pub fn from_arg(arg: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.arg() == arg)
    }

    fn bus(self) -> &'static str {
        match self {
            Self::Microphone | Self::WiFi => "pci",
            Self::Camera | Self::Bluetooth => "usb",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Microphone => "audio",
            Self::Camera => "cam",
            Self::WiFi => "net",
            Self::Bluetooth => "bt",
        }
    }
}

/// Whether each device is unblocked, or `None` when the target has no
/// devices with its tag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status([Option<bool>; Device::ALL.len()]);

impl Status {
    pub fn get(self, device: Device) -> Option<bool> {
        self.0[device as usize]
    }

    pub fn set(&mut self, device: Device, state: Option<bool>) {
        self.0[device as usize] = state;
    }

    /// Set every present device to `enabled`.
    pub fn set_all(&mut self, enabled: bool) {
        for state in self.0.iter_mut().flatten() {
            *state = enabled;
        }
    }

    pub fn present(self) -> impl Iterator<Item = (Device, bool)> {
        Device::ALL
            .into_iter()
            .filter_map(move |d| self.get(d).map(|enabled| (d, enabled)))
    }

    pub fn all_blocked(self) -> bool {
        self.present().all(|(_, enabled)| !enabled)
    }
}

/// Whether `device` is unblocked: at least one device with its tag is
/// attached to a VM. `None` when there are no such devices.
async fn device_state(port: u32, device: Device) -> Result<Option<bool>, String> {
    let resp = api::call(
        port,
        json!({"action": format!("{}_list", device.bus()), "tag": device.tag()}),
    )
    .await
    .map_err(|e| format!("Failed to read {} status: {e}", device.arg()))?;
    let listed = resp.pci_devices.or(resp.usb_devices).unwrap_or_default();
    Ok((!listed.is_empty()).then(|| listed.iter().any(|d| d.vm.is_some())))
}

pub async fn status(port: u32) -> Result<Status, String> {
    let states = join_all(Device::ALL.map(|d| device_state(port, d))).await;
    let mut status = Status::default();
    for (device, state) in Device::ALL.into_iter().zip(states) {
        status.set(device, state?);
    }
    Ok(status)
}

/// Attach (`enabled`) or detach every device with the tag of `device`.
pub async fn set(port: u32, device: Device, enabled: bool) -> Result<(), String> {
    let action = if enabled { "attach" } else { "detach" };
    api::call(
        port,
        json!({"action": format!("{}_{action}", device.bus()), "tag": device.tag()}),
    )
    .await
    .map(|_| ())
    .map_err(|e| {
        let verb = if enabled { "unblock" } else { "block" };
        format!("Failed to {verb} {}: {e}", device.arg())
    })
}

/// Set every present device that is not already in that state.
pub async fn set_all(port: u32, enabled: bool) -> Result<(), String> {
    let status = status(port).await?;
    let changes = status
        .present()
        .filter(|&(_, state)| state != enabled)
        .map(|(device, _)| set(port, device, enabled));
    let errors: Vec<String> = join_all(changes)
        .await
        .into_iter()
        .filter_map(Result::err)
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Whether `event` reports a device being attached to or detached from a VM.
fn is_attach_change(event: &str) -> bool {
    matches!(
        event,
        "pci_attached" | "pci_detached" | "usb_attached" | "usb_detached"
    )
}

/// Endless stream that yields whenever a device is attached to or detached
/// from a VM.
pub fn changes(port: &u32) -> impl Stream<Item = ()> + use<> {
    api::events(*port)
        .filter(|event| future::ready(is_attach_change(event)))
        .map(|_| ())
}
