// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{image, process};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    process::Command,
};

#[derive(Parser)]
#[command(about = "Create a Ghaf A/B LVM layout in a regular file without kernel devices")]
pub struct Options {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    image: Option<PathBuf>,
    #[arg(long)]
    root_size_mib: u64,
    #[arg(long)]
    verity_size_mib: u64,
    #[arg(long)]
    create_inactive_slots: bool,
    #[arg(long, default_value_t = 0)]
    swap_size_mib: u64,
    #[arg(long, default_value_t = 0)]
    persist_size_mib: u64,
    #[arg(long)]
    print_plan: bool,
}

#[derive(Deserialize)]
struct Manifest {
    manifest_version: u64,
    root: Artifact,
    verity: Artifact,
}

#[derive(Deserialize)]
struct Artifact {
    file: String,
    unpacked_size: u64,
}

// Creation-only LVM2 format-text: one PV, one MDA, contiguous single-stripe LVs.
// See upstream lib/label/label.h and lib/format_text/layout.h. All disk integers
// are little-endian; text sizes are in 512-byte sectors, header sizes in bytes.
const PE_START: u64 = 5 * image::MIB;
const PE_SIZE: u64 = 4 * image::MIB;
const MDA_START: u64 = 4096;

struct Volume {
    name: String,
    capacity: u64,
    offset: u64,
}

impl Volume {
    fn end(&self) -> Result<u64> {
        self.capacity
            .checked_next_multiple_of(PE_SIZE)
            .and_then(|size| self.offset.checked_add(size))
            .context("LVM extent overflow")
    }
}

struct Layout {
    manifest: Manifest,
    directory: PathBuf,
    suffix: String,
    volumes: Vec<Volume>,
    minimum_mib: u64,
}

impl Layout {
    fn new(options: &Options) -> Result<Self> {
        let path = image::regular_file(&options.manifest)?;
        let directory = path.parent().context("manifest has no parent")?.to_owned();
        let manifest: Manifest = serde_json::from_reader(File::open(&path)?)?;
        ensure!(manifest.manifest_version == 2, "manifest_version must be 2");
        ensure!(
            options.root_size_mib > 0 && options.verity_size_mib > 0,
            "slot capacities must be positive"
        );
        let suffix = manifest
            .root
            .file
            .strip_prefix("ghaf_root_")
            .and_then(|name| name.strip_suffix(".raw.zst"))
            .context("Invalid root artifact name")?
            .to_owned();
        image::safe_name(&suffix)?;
        ensure!(
            manifest.verity.file == format!("ghaf_verity_{suffix}.raw.zst"),
            "Root and verity artifact slot names do not match"
        );
        let mut layout = Self {
            manifest,
            directory,
            suffix,
            volumes: Vec::new(),
            minimum_mib: 64,
        };
        let mut slots = vec![
            (format!("root_{}", layout.suffix), options.root_size_mib),
            (format!("verity_{}", layout.suffix), options.verity_size_mib),
        ];
        if options.create_inactive_slots {
            slots.extend([
                ("root_empty".into(), options.root_size_mib),
                ("verity_empty".into(), options.verity_size_mib),
            ]);
        }
        slots.extend([
            ("swap".into(), options.swap_size_mib),
            ("persist".into(), options.persist_size_mib),
        ]);
        let mut offset = PE_START;
        for (name, size) in slots {
            if size == 0 {
                continue;
            }
            volume_name(&name)?;
            ensure!(
                !layout.volumes.iter().any(|v| v.name == name),
                "duplicate LV"
            );
            let volume = Volume {
                name,
                capacity: image::mib(size)?,
                offset,
            };
            offset = volume.end()?;
            layout.minimum_mib = layout
                .minimum_mib
                .checked_add(size)
                .context("image capacity overflow")?;
            layout.volumes.push(volume);
        }
        image::mib(layout.minimum_mib)?;
        for (volume, artifact) in layout.payloads() {
            ensure!(
                layout.directory.join(&artifact.file).is_file(),
                "Missing update artifact: {}",
                artifact.file
            );
            ensure!(
                artifact.unpacked_size > 0 && artifact.unpacked_size <= volume.capacity,
                "{} does not fit its slot",
                artifact.file
            );
        }
        Ok(layout)
    }

    fn payloads(&self) -> impl Iterator<Item = (&Volume, &Artifact)> {
        self.volumes
            .iter()
            .zip([&self.manifest.root, &self.manifest.verity])
    }

    fn plan(&self) -> serde_json::Value {
        // Retain the external plan JSON, without a second internal layout model.
        serde_json::json!({
            "root_file": self.manifest.root.file,
            "verity_file": self.manifest.verity.file,
            "lv_suffix": self.suffix,
            "root_size_mib": self.volumes[0].capacity / image::MIB,
            "verity_size_mib": self.volumes[1].capacity / image::MIB,
            "minimum_pv_size_mib": self.minimum_mib,
        })
    }

    fn populate(&self, target: &mut File) -> Result<()> {
        let size = target.metadata()?.len();
        ensure!(
            size.is_multiple_of(512) && size >= image::mib(self.minimum_mib)?,
            "Image is too small or not sector-aligned"
        );
        for (volume, artifact) in self.payloads() {
            let copied = process::consume(
                Command::new("zstd")
                    .args(["--decompress", "--stdout"])
                    .arg(self.directory.join(&artifact.file)),
                |stream| image::copy_bounded(stream, target, volume.offset, volume.capacity, false),
            )
            .with_context(|| {
                format!(
                    "Payload for {} exceeds its {} MiB logical volume or failed to decompress",
                    volume.name,
                    volume.capacity / image::MIB
                )
            })?;
            ensure!(
                copied == artifact.unpacked_size,
                "{}: expected {} unpacked bytes, got {copied}",
                artifact.file,
                artifact.unpacked_size
            );
        }
        for volume in &self.volumes[2..] {
            let formatter = match volume.name.as_str() {
                "swap" => "mkswap",
                "persist" => "mkfs.btrfs",
                _ => continue,
            };
            let temporary = tempfile::NamedTempFile::new()?;
            temporary.as_file().set_len(volume.capacity)?;
            let mut command = Command::new(formatter);
            if formatter == "mkfs.btrfs" {
                command.arg("--force");
            }
            process::run(
                command
                    .args(["--label", &volume.name])
                    .arg(temporary.path()),
            )?;
            image::copy_bounded(
                &mut File::open(temporary.path())?,
                target,
                volume.offset,
                volume.capacity,
                true,
            )?;
        }
        self.write_metadata(target)?;
        target.sync_all()?;
        Ok(())
    }

    fn write_metadata(&self, target: &mut File) -> Result<()> {
        let size = target.metadata()?.len();
        let id = |suffix: &str| identifier(&format!("pool:{suffix}"));
        let pv_id = id("pv");
        let epoch: u64 = std::env::var("SOURCE_DATE_EPOCH")
            .unwrap_or_else(|_| "1".into())
            .parse()?;
        let mut metadata = format!(
            r#"pool {{
id = "{}"
seqno = 1
format = "lvm2"
status = ["RESIZEABLE", "READ", "WRITE"]
flags = []
extent_size = {extent_sectors}
max_lv = 0
max_pv = 0
metadata_copies = 0
physical_volumes {{
pv0 {{
id = "{pv_id}"
device = "/dev/ghaf-image"
status = ["ALLOCATABLE"]
flags = []
dev_size = {}
pe_start = {start_sectors}
pe_count = {}
}}
}}
logical_volumes {{
"#,
            id("vg"),
            size / 512,
            (size - PE_START) / PE_SIZE,
            extent_sectors = PE_SIZE / 512,
            start_sectors = PE_START / 512
        );
        for volume in &self.volumes {
            let name = &volume.name;
            let first = (volume.offset - PE_START) / PE_SIZE;
            let count = volume.capacity.div_ceil(PE_SIZE);
            metadata.push_str(&format!(
                r#"{name} {{
id = "{}"
status = ["READ", "WRITE", "VISIBLE"]
flags = []
creation_time = {epoch}
creation_host = "ghaf-image"
segment_count = 1
segment1 {{
start_extent = 0
extent_count = {count}
type = "striped"
stripe_count = 1
stripes = ["pv0", {first}]
}}
}}
"#,
                id(&format!("lv:{name}"))
            ));
        }
        metadata.push_str(&format!(
            r#"}}
}}
contents = "Text Format Volume Group"
version = 1
description = "ghaf-image-builder"
creation_host = "ghaf-image"
creation_time = {epoch}
"#
        ));
        metadata.push('\0');
        let mda_size = PE_START - MDA_START;
        // Leave room for a second metadata copy when stock LVM updates the VG.
        ensure!(
            2 * metadata.len() as u64 + 512 <= mda_size,
            "LVM metadata exceeds area"
        );
        let label = sector(
            &[
                (0, b"LABELONE"),
                (8, &1u64.to_le_bytes()),
                (20, &32u32.to_le_bytes()),
                (24, b"LVM2 001"),
                (32, pv_id.replace('-', "").as_bytes()),
                (64, &size.to_le_bytes()),
                (72, &PE_START.to_le_bytes()),
                (104, &MDA_START.to_le_bytes()),
                (112, &mda_size.to_le_bytes()),
                (136, &2u32.to_le_bytes()),
                (140, &1u32.to_le_bytes()),
            ],
            20,
        );
        let header = sector(
            &[
                (4, b" LVM2 x[5A%r0N*>"),
                (20, &1u32.to_le_bytes()),
                (24, &MDA_START.to_le_bytes()),
                (32, &mda_size.to_le_bytes()),
                (40, &512u64.to_le_bytes()),
                (48, &(metadata.len() as u64).to_le_bytes()),
                (56, &crc(metadata.as_bytes()).to_le_bytes()),
            ],
            4,
        );
        for (offset, bytes) in [
            (512, label.as_slice()),
            (MDA_START, &header),
            (MDA_START + 512, metadata.as_bytes()),
        ] {
            target.seek(SeekFrom::Start(offset))?;
            target.write_all(bytes)?;
        }
        Ok(())
    }
}

impl Options {
    pub fn run(&self) -> Result<()> {
        let layout = Layout::new(self)?;
        if self.print_plan {
            println!("{}", layout.plan());
            return Ok(());
        }
        let path = image::regular_file(
            self.image
                .as_deref()
                .context("--image is required unless --print-plan is used")?,
        )?;
        layout.populate(&mut OpenOptions::new().read(true).write(true).open(&path)?)?;
        println!(
            "Initialized Ghaf verity LVM volume group pool in {}",
            path.display()
        );
        Ok(())
    }
}

fn volume_name(name: &str) -> Result<()> {
    image::safe_name(name)?;
    ensure!(
        name.len() < 128 && !name.starts_with('-'),
        "invalid LVM name"
    );
    Ok(())
}

fn sector(fields: &[(usize, &[u8])], checksum_start: usize) -> [u8; 512] {
    let mut bytes = [0; 512];
    for &(offset, value) in fields {
        bytes[offset..offset + value.len()].copy_from_slice(value);
    }
    let checksum = crc(&bytes[checksum_start..]);
    bytes[checksum_start - 4..checksum_start].copy_from_slice(&checksum.to_le_bytes());
    bytes
}

fn crc(bytes: &[u8]) -> u32 {
    let mut value = 0xf597_a6cf;
    for byte in bytes {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(value & 1));
        }
    }
    value
}

// Preserve the deterministic identifiers used by the previous offline builder.
fn identifier(seed: &str) -> String {
    let mix = |state: u64, byte: u64| (state ^ byte).wrapping_mul(1099511628211);
    let mut state = seed
        .bytes()
        .fold(14695981039346656037, |s, b| mix(s, b.into()));
    let mut id = String::new();
    for i in 0..32 {
        if [6, 10, 14, 18, 22, 26].contains(&i) {
            id.push('-');
        }
        state = mix(state, i);
        id.push(
            b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"[(state % 62) as usize]
                as char,
        );
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_rounding_and_overflow() {
        let mut volume = Volume {
            name: "root".into(),
            capacity: image::MIB,
            offset: PE_START,
        };
        assert_eq!(volume.end().unwrap(), 9 * image::MIB);
        volume.capacity = 5 * image::MIB;
        assert_eq!(volume.end().unwrap(), 13 * image::MIB);
        volume.capacity = u64::MAX;
        assert!(volume.end().is_err());
        volume.capacity = PE_SIZE;
        volume.offset = u64::MAX;
        assert!(volume.end().is_err());
        assert!(volume_name("-root").is_err());
        assert!(volume_name(&"x".repeat(128)).is_err());
        assert_eq!(
            identifier("ghaf_test:pv"),
            "C5RAaT-JqAL-VgWR-72Ez-3wSh-DWkh-fQeDvI"
        );
    }

    #[test]
    fn rejects_boolean_sizes() {
        assert!(serde_json::from_str::<Manifest>(
            r#"{"manifest_version":2,"root":{"file":"root","unpacked_size":true},"verity":{"file":"verity","unpacked_size":1}}"#
        ).is_err());
    }
}
