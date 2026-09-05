// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{image, process};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Parser)]
#[command(about = "Wrap a sparse regular-file image in LUKS2 without kernel devices")]
pub struct Options {
    #[arg(long)]
    pub image: PathBuf,
    #[arg(long)]
    pub uuid: String,
    #[arg(long)]
    pub key_file: PathBuf,
    #[arg(long, default_value_t = 32)]
    pub header_size_mib: u64,
}

impl Options {
    pub fn run(&self) -> Result<()> {
        validate_uuid(&self.uuid)?;
        ensure!(
            self.header_size_mib >= 16,
            "LUKS2 conversion needs at least 16 MiB of header space"
        );
        let image = image::regular_file(&self.image)?;
        let extents = image::data_extents(&File::open(&image)?)?;
        let key_file = image::regular_file(&self.key_file)?;
        let header_size = image::mib(self.header_size_mib)?;
        let size = image
            .metadata()?
            .len()
            .checked_add(header_size)
            .context("LUKS image size overflow")?;
        let cryptsetup = std::env::var_os("GHAF_CRYPTSETUP_OFFLINE")
            .context("GHAF_CRYPTSETUP_OFFLINE is not configured")?;

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
                let mut command = Command::new(&cryptsetup);
                // Every header operation below targets our private temporary
                // file. No other writer can access it; no shared /tmp lock path.
                command.args(["--disable-locks", action]);
                command
            };
            process::run(
                crypt("luksFormat")
                    .args([
                        "--batch-mode",
                        "--type=luks1",
                        // This temporary passphrase has 256 random bits, not
                        // human entropy. Avoid repeating a costly KDF for each
                        // extent. The caller's final key uses cryptsetup defaults.
                        "--pbkdf-force-iterations=1000",
                        "--align-payload",
                        &(header_size / 512).to_string(),
                        "--uuid",
                        &self.uuid,
                        "--key-file",
                    ])
                    .arg(construction_key.path())
                    .arg(temporary),
            )?;

            // Raw slices ABOVE the LUKS driver preserve absolute crypto sector
            // numbers. Copy all bytes within each data extent, including zeroes;
            // qemu-img's content-based sparse detection must not skip them.
            let secret = format!(
                "secret,id=construction,file={}",
                qemu_path(construction_key.path())?
            );
            for extent in &extents {
                let window = format!(
                    "driver=raw,offset={},size={}",
                    extent.start,
                    extent.end - extent.start
                );
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

            process::run(crypt("convert").args(["--type", "luks2"]).arg(temporary))?;
            process::run(
                crypt("luksAddKey")
                    .args(["--batch-mode", "--key-file"])
                    .arg(construction_key.path())
                    .arg(temporary)
                    .arg(&key_file),
            )?;
            process::run(
                crypt("luksRemoveKey")
                    .args(["--batch-mode", "--key-file"])
                    .arg(construction_key.path())
                    .arg(temporary),
            )?;
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
                offset == header_size,
                "LUKS payload offset mismatch: expected {header_size}, got {offset}"
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

fn validate_uuid(uuid: &str) -> Result<()> {
    let parts = uuid.split('-').collect::<Vec<_>>();
    ensure!(
        parts.len() == 5
            && parts
                .iter()
                .zip([8, 4, 4, 4, 12])
                .all(|(part, length)| part.len() == length
                    && part.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "invalid LUKS UUID"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_and_option_validation() {
        assert!(validate_uuid("01234567-89AB-4cde-8fab-0123456789ab").is_ok());
        assert!(validate_uuid("01234567-89ab-4cde-8fab-0123456789az").is_err());
        assert!(validate_uuid("01234567-89ab-4cde-8fab-0123456789ab-extra").is_err());
        assert_eq!(
            qemu_path(Path::new("/tmp/with,comma")).unwrap(),
            "/tmp/with,,comma"
        );
    }
}
