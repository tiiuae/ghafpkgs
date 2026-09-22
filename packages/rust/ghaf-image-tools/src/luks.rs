// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{image, process};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use serde::Deserialize;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

const HEADER_SIZE: u64 = 32 * image::MIB;

#[derive(Deserialize)]
struct Extent {
    start: u64,
    length: u64,
    data: bool,
}

#[derive(Parser)]
#[command(about = "Wrap a sparse regular-file image in LUKS2 without kernel devices")]
pub struct Options {
    #[arg(long)]
    image: PathBuf,
    #[arg(long)]
    uuid: String,
    #[arg(long)]
    key_file: PathBuf,
}

impl Options {
    pub fn run(&self) -> Result<()> {
        let image = image::regular_file(&self.image)?;
        let input = File::open(&image)?;
        let input_size = input.metadata()?.len();
        ensure!(
            input_size > 0 && input_size.is_multiple_of(512),
            "image size must be a positive multiple of 512 bytes"
        );
        input.sync_all()?;
        let extents: Vec<Extent> = serde_json::from_str(&process::output(
            Command::new("qemu-img")
                .args(["map", "-f", "raw", "--output=json"])
                .arg(&image),
        )?)?;
        let mut end = 0;
        for extent in &extents {
            ensure!(
                extent.start == end
                    && extent.length > 0
                    && extent.length <= input_size - end
                    && extent.length.is_multiple_of(512),
                "invalid or unaligned QEMU image extent"
            );
            end += extent.length;
        }
        ensure!(end == input_size, "incomplete QEMU image map");
        let key_file = image::regular_file(&self.key_file)?;
        let size = input_size
            .checked_add(HEADER_SIZE)
            .context("LUKS image size overflow")?;
        let mut volume_key = tempfile::NamedTempFile::new()?;
        let mut random = [0; 64];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        volume_key.write_all(&random)?;
        volume_key.flush()?;
        let mut construction_key = tempfile::NamedTempFile::new()?;
        let mut random = [0; 32];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        for byte in random {
            write!(construction_key, "{byte:02x}")?;
        }
        construction_key.flush()?;

        image::replace_image(&image, |temporary| {
            File::options().write(true).open(temporary)?.set_len(size)?;
            let crypt = |action: &str| {
                let mut command = Command::new("cryptsetup");
                // Every header operation below targets our private temporary
                // file. No other writer can access it; no shared /tmp lock path.
                command.args(["--disable-locks", action]);
                command
            };
            let format = |kind: &str, key: &Path| {
                let mut command = crypt("luksFormat");
                command
                    .args([
                        "--batch-mode",
                        "--type",
                        kind,
                        "--cipher=aes-xts-plain64",
                        "--key-size=512",
                        "--offset",
                        &(HEADER_SIZE / 512).to_string(),
                        "--uuid",
                        &self.uuid,
                        "--volume-key-file",
                    ])
                    .arg(volume_key.path())
                    .arg("--key-file")
                    .arg(key)
                    .arg(temporary);
                command
            };
            // The random construction passphrase permits a cheap KDF for each
            // QEMU extent. The final header uses the caller's key and default KDF.
            process::run(
                format("luks1", construction_key.path()).arg("--pbkdf-force-iterations=1000"),
            )?;

            // Raw slices ABOVE the LUKS driver preserve absolute crypto sector
            // numbers. Copy all bytes within each data extent, including zeroes;
            // qemu-img's content-based sparse detection must not skip them.
            let secret = format!(
                "secret,id=construction,file={}",
                qemu_path(construction_key.path())?
            );
            for extent in extents.iter().filter(|extent| extent.data) {
                let window = format!("driver=raw,offset={},size={}", extent.start, extent.length);
                let source = format!(
                    "{window},file.driver=file,file.filename={}",
                    qemu_path(&image)?
                );
                let target = format!(
                    "{window},file.driver=luks,file.key-secret=construction,file.file.driver=file,file.file.filename={}",
                    qemu_path(temporary)?
                );
                process::run(Command::new("qemu-img").args([
                    "convert",
                    "--object",
                    &secret,
                    "--no-create",
                    "--image-opts",
                    "--target-image-opts",
                    "--sparse-size=0",
                    &source,
                    &target,
                ]))?;
            }

            // Replace only the header, retaining the volume key and payload
            // parameters. Unlike convert, luksFormat needs no device-mapper check.
            process::run(format("luks2", &key_file).arg("--sector-size=512"))?;
            process::run(crypt("isLuks").args(["--type", "luks2"]).arg(temporary))?;
            ensure!(
                process::output(crypt("luksUUID").arg(temporary))?.trim()
                    == self.uuid.to_ascii_lowercase(),
                "LUKS UUID mismatch"
            );
            let metadata: serde_json::Value = serde_json::from_str(&process::output(
                crypt("luksDump").arg("--dump-json-metadata").arg(temporary),
            )?)?;
            let offset: u64 = metadata["segments"]["0"]["offset"]
                .as_str()
                .context("missing LUKS payload offset")?
                .parse()?;
            ensure!(
                offset == HEADER_SIZE,
                "LUKS payload offset mismatch: expected {HEADER_SIZE}, got {offset}"
            );
            process::run(
                crypt("open")
                    .args(["--test-passphrase", "--key-file"])
                    .arg(&key_file)
                    .arg(temporary),
            )?;
            Ok(())
        })
    }
}

fn qemu_path(path: &Path) -> Result<String> {
    // QEMU's comma-separated option syntax escapes a literal comma by doubling it.
    Ok(path
        .to_str()
        .context("QEMU path must be UTF-8")?
        .replace(',', ",,"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_option_escaping() {
        assert_eq!(
            qemu_path(Path::new("/tmp/with,comma")).unwrap(),
            "/tmp/with,,comma"
        );
    }
}
