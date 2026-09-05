// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

pub mod image;
pub mod luks;
pub mod lvm;
pub mod process;

pub fn exit_on_error(result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
