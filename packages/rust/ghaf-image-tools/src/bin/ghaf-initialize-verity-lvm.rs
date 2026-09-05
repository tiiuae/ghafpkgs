// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

fn main() {
    ghaf_image_tools::exit_on_error(ghaf_image_tools::lvm::Options::parse().run());
}
