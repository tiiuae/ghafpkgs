// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{image, process};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[derive(Debug, Parser)]
#[command(about = "Create a Ghaf A/B LVM layout in a regular file without kernel devices")]
pub struct Options {
    #[arg(long)]
    pub update_dir: PathBuf,
    #[arg(long)]
    pub image: Option<PathBuf>,
    #[arg(long)]
    pub root_size_mib: u64,
    #[arg(long)]
    pub verity_size_mib: u64,
    #[arg(long, default_value = "pool")]
    pub vg_name: String,
    #[arg(long)]
    pub create_inactive_slots: bool,
    #[arg(long, default_value_t = 0)]
    pub swap_size_mib: u64,
    #[arg(long, default_value_t = 0)]
    pub persist_size_mib: u64,
    #[arg(long)]
    pub print_plan: bool,
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

/// The serialized fields preserve the existing --print-plan contract.
#[derive(Debug, Serialize)]
pub struct Plan {
    root_file: String,
    verity_file: String,
    lv_suffix: String,
    root_size_mib: u64,
    verity_size_mib: u64,
    minimum_pv_size_mib: u64,
}

impl Plan {
    pub fn new(options: &Options) -> Result<Self> {
        image::safe_name(&options.vg_name)?;
        ensure!(
            options.root_size_mib > 0 && options.verity_size_mib > 0,
            "slot capacities must be positive"
        );
        let mut manifest = None;
        for entry in fs::read_dir(&options.update_dir)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "manifest")
                && entry.file_type()?.is_file()
            {
                ensure!(
                    manifest.replace(entry.path()).is_none(),
                    "multiple update manifests"
                );
            }
        }
        let manifest: Manifest =
            serde_json::from_reader(File::open(manifest.context("missing update manifest")?)?)
                .context("invalid image manifest")?;
        ensure!(manifest.manifest_version == 2, "manifest_version must be 2");
        for (artifact, capacity) in [
            (&manifest.root, options.root_size_mib),
            (&manifest.verity, options.verity_size_mib),
        ] {
            image::safe_name(&artifact.file)?;
            ensure!(
                options.update_dir.join(&artifact.file).is_file(),
                "Missing update artifact: {}",
                artifact.file
            );
            ensure!(
                artifact.unpacked_size > 0 && artifact.unpacked_size <= image::mib(capacity)?,
                "{} does not fit its {capacity} MiB slot",
                artifact.file
            );
        }
        let suffix = manifest
            .root
            .file
            .strip_prefix("ghaf_root_")
            .and_then(|name| name.strip_suffix(".raw.zst"))
            .context("Cannot derive a safe LVM slot suffix")?;
        image::safe_name(suffix)?;
        ensure!(
            manifest.verity.file == format!("ghaf_verity_{suffix}.raw.zst"),
            "Root and verity artifact slot names do not match"
        );
        let pair = options
            .root_size_mib
            .checked_add(options.verity_size_mib)
            .context("slot capacity overflow")?;
        let copies = if options.create_inactive_slots { 2 } else { 1 };
        let minimum = pair
            .checked_mul(copies)
            .and_then(|size| size.checked_add(options.swap_size_mib))
            .and_then(|size| size.checked_add(options.persist_size_mib))
            .and_then(|size| size.checked_add(64))
            .context("image capacity overflow")?;
        image::mib(minimum)?;
        Ok(Self {
            lv_suffix: suffix.to_owned(),
            root_file: manifest.root.file,
            verity_file: manifest.verity.file,
            root_size_mib: options.root_size_mib,
            verity_size_mib: options.verity_size_mib,
            minimum_pv_size_mib: minimum,
        })
    }
}

impl Options {
    pub fn run(&self) -> Result<()> {
        let plan = Plan::new(self)?;
        if self.print_plan {
            println!("{}", serde_json::to_string(&plan)?);
            return Ok(());
        }
        let path = image::regular_file(
            self.image
                .as_deref()
                .context("--image is required unless --print-plan is used")?,
        )?;
        // LVM's config language embeds this name in a quoted string.
        let name = path.to_str().context("image path must be UTF-8")?;
        ensure!(
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_./:+-".contains(&byte)),
            "Image path contains characters unsupported by the LVM configuration syntax"
        );
        let mut target = OpenOptions::new().read(true).write(true).open(&path)?;
        ensure!(
            target.metadata()?.len() >= image::mib(plan.minimum_pv_size_mib)?,
            "Image is too small: need at least {} MiB",
            plan.minimum_pv_size_mib
        );
        let lvm = Lvm::new(&path, &self.vg_name)?;
        process::run(
            lvm.command("pvcreate")
                .env("LVM_OFFLINE_UUID_SEED", format!("{}:pv", self.vg_name))
                .args([
                    "--yes",
                    "--force",
                    "--force",
                    "--zero=y",
                    "--metadatasize=4M",
                    "--dataalignment=1M",
                ])
                .arg(&path),
        )?;
        process::run(
            lvm.command("vgcreate")
                .env("LVM_OFFLINE_UUID_SEED", format!("{}:vg", self.vg_name))
                .args(["--yes", "--physicalextentsize", "4M", &self.vg_name])
                .arg(&path),
        )?;
        for (kind, file, size) in [
            ("root", &plan.root_file, plan.root_size_mib),
            ("verity", &plan.verity_file, plan.verity_size_mib),
        ] {
            let lv = format!("{kind}_{}", plan.lv_suffix);
            lvm.create(&lv, size)?;
            let offset = lvm.offset(&lv)?;
            process::consume(
                Command::new("zstd")
                    .args(["--decompress", "--stdout"])
                    .arg(self.update_dir.join(file)),
                |stream| image::copy_bounded(stream, &mut target, offset, image::mib(size)?, false),
            )
            .with_context(|| {
                format!(
                    "Payload for {lv} exceeds its {size} MiB logical volume or failed to decompress"
                )
            })?;
        }
        if self.create_inactive_slots {
            lvm.create("root_empty", plan.root_size_mib)?;
            lvm.create("verity_empty", plan.verity_size_mib)?;
        }
        for (lv, size, formatter) in [
            ("swap", self.swap_size_mib, "mkswap"),
            ("persist", self.persist_size_mib, "mkfs.btrfs"),
        ] {
            if size == 0 {
                continue;
            }
            lvm.create(lv, size)?;
            let temporary = lvm.work.path().join(format!("{lv}.img"));
            File::create(&temporary)?.set_len(image::mib(size)?)?;
            let mut command = Command::new(formatter);
            if lv == "persist" {
                command.arg("--force");
            }
            process::run(command.args(["--label", lv]).arg(&temporary))?;
            image::copy_bounded(
                &mut File::open(temporary)?,
                &mut target,
                lvm.offset(lv)?,
                image::mib(size)?,
                true,
            )?;
        }
        process::run(lvm.command("vgck").arg(&self.vg_name))?;
        target.sync_all()?;
        println!(
            "Initialized Ghaf verity LVM volume group {} in {}",
            self.vg_name,
            path.display()
        );
        Ok(())
    }
}

struct Lvm {
    binary: PathBuf,
    work: TempDir,
    config: String,
    image: PathBuf,
    vg: String,
}

impl Lvm {
    fn new(path: &Path, vg: &str) -> Result<Self> {
        let work = tempfile::tempdir()?;
        for directory in ["dev", "lvm/archive", "lvm/backup"] {
            fs::create_dir_all(work.path().join(directory))?;
        }
        // Reject characters that could change the generated LVM configuration.
        let dev = work.path().join("dev");
        ensure!(
            !dev.to_string_lossy().contains(['"', '\\', '\n']),
            "unsupported temporary directory"
        );
        Ok(Self {
            binary: std::env::var_os("GHAF_LVM_OFFLINE")
                .context("GHAF_LVM_OFFLINE is not configured")?
                .into(),
            config: format!(
                "devices {{ loopfiles = [ \"{}\" ] scan = [ \"{}\" ] use_devicesfile = 0 obtain_device_list_from_udev = 0 sysfs_scan = 0 }} global {{ locking_type = 0 }} activation {{ udev_sync = 0 udev_rules = 0 }} backup {{ backup = 0 archive = 0 }}",
                path.display(),
                dev.display()
            ),
            work,
            image: path.to_owned(),
            vg: vg.to_owned(),
        })
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .arg(action)
            .args(["--driverloaded=n", "--nolocking", "--config", &self.config])
            .env("LVM_SYSTEM_DIR", self.work.path().join("lvm"))
            .env("LVM_OFFLINE_HOST", "ghaf-image")
            .env("LVM_OFFLINE_DESCRIPTION", "ghaf-image-builder")
            .env("LVM_OFFLINE_DEVICE_HINT", "/dev/ghaf-image")
            .env(
                "SOURCE_DATE_EPOCH",
                std::env::var_os("SOURCE_DATE_EPOCH").unwrap_or_else(|| "1".into()),
            )
            .env("TZ", "UTC");
        command
    }

    fn create(&self, name: &str, size: u64) -> Result<()> {
        process::run(
            self.command("lvcreate")
                .env("LVM_OFFLINE_UUID_SEED", format!("{}:lv:{name}", self.vg))
                .args(["--yes", "--activate=n", "--zero=n", "--wipesignatures=n"])
                .args(["--size", &format!("{size}M"), "--name", name, &self.vg]),
        )
    }

    fn offset(&self, lv: &str) -> Result<u64> {
        extent_offset(
            self.report("pvs", "pe_start", &self.image)?.parse()?,
            self.report("vgs", "vg_extent_size", &self.vg)?.parse()?,
            &self.image,
            &self.report("lvs", "seg_pe_ranges", format!("{}/{lv}", self.vg))?,
        )
    }

    fn report(
        &self,
        command: &str,
        field: &str,
        target: impl AsRef<std::ffi::OsStr>,
    ) -> Result<String> {
        let mut query = self.command(command);
        query.args(["--noheadings", "--units=b", "--nosuffix", "-o", field]);
        if command == "lvs" {
            query.arg("--segments");
        }
        Ok(process::output(query.arg(target))?.trim().to_owned())
    }
}

fn extent_offset(start: u64, extent: u64, image: &Path, ranges: &str) -> Result<u64> {
    let (device, range) = ranges
        .rsplit_once(':')
        .context("invalid LVM extent report")?;
    ensure!(
        Path::new(device) == image,
        "LVM reported an unexpected device"
    );
    let (first, last) = range.split_once('-').context("invalid LVM extent range")?;
    let first: u64 = first.parse()?;
    let last: u64 = last.parse()?;
    ensure!(extent > 0 && first <= last, "invalid LVM extent bounds");
    first
        .checked_mul(extent)
        .and_then(|offset| start.checked_add(offset))
        .context("LVM extent offset overflow")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::MIB;

    #[test]
    fn rejects_wrong_device_fragmented_and_overflowing_extent_reports() {
        let image = Path::new("/image");
        assert_eq!(
            extent_offset(5 * MIB, 4 * MIB, image, "/image:1-2").unwrap(),
            9 * MIB
        );
        for report in [
            "/other:1-2",
            "/image:1-2 /image:3-4",
            "/image:3-1",
            "/image:invalid",
        ] {
            assert!(extent_offset(0, 4 * MIB, image, report).is_err());
        }
        assert!(extent_offset(1, u64::MAX, image, "/image:2-3").is_err());
    }

    #[test]
    fn rejects_boolean_sizes() {
        assert!(serde_json::from_str::<Manifest>(
            r#"{"manifest_version":2,"root":{"file":"root","unpacked_size":true},"verity":{"file":"verity","unpacked_size":1}}"#
        ).is_err());
    }
}
