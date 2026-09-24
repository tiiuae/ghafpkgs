/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
mod api;
mod app;
mod config;
mod killswitch;

pub use api::DEFAULT_PORT;

pub fn run(port: u32) -> cosmic::iced::Result {
    app::run(port)
}
