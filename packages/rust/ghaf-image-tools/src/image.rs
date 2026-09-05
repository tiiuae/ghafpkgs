// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const MIB: u64 = 1024 * 1024;

pub fn mib(value: u64) -> Result<u64> {
    value
        .checked_mul(MIB)
        .context("MiB size exceeds supported byte range")
}

pub fn regular_file(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("resolve {}", path.display()))?;
    ensure!(
        path.metadata()?.is_file(),
        "{} must be a regular file",
        path.display()
    );
    Ok(path)
}

/// Writes never pass the declared boundary, even if the producer lies about its
/// size. Sparse copying is only valid when skipped target regions are zero.
pub fn copy_bounded(
    source: &mut dyn Read,
    target: &mut File,
    offset: u64,
    capacity: u64,
    sparse: bool,
) -> Result<u64> {
    let end = offset
        .checked_add(capacity)
        .context("image extent overflow")?;
    ensure!(end <= target.metadata()?.len(), "extent exceeds image size");
    target.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0; MIB as usize];
    let mut copied = 0;
    while copied < capacity {
        let limit = (capacity - copied).min(buffer.len() as u64) as usize;
        let read = source.read(&mut buffer[..limit])?;
        if read == 0 {
            return Ok(copied);
        }
        if sparse && buffer[..read].iter().all(|byte| *byte == 0) {
            target.seek(SeekFrom::Current(read as i64))?;
        } else {
            target.write_all(&buffer[..read])?;
        }
        copied += read as u64;
    }
    let mut extra = [0];
    ensure!(
        source.read(&mut extra)? == 0,
        "payload exceeds its logical volume"
    );
    Ok(copied)
}

/// Construct beside the destination and replace it only after all checks pass.
/// Before publication, errors or termination leave the original image intact.
/// A killed process may leave a temporary file; directory sync can fail after
/// publication and cannot roll back the completed rename.
pub fn replace_image(image: &Path, construct: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let parent = image.parent().context("image has no parent directory")?;
    let temporary = tempfile::Builder::new()
        .prefix(".ghaf-image-")
        .tempfile_in(parent)?;
    construct(temporary.path())?;
    temporary
        .as_file()
        .set_permissions(fs::metadata(image)?.permissions())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(image)
        .map_err(|error| error.error)
        .context("publish image")?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn safe_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_+.-".contains(&byte))
    {
        bail!("unsafe image or logical-volume name: {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn overlong_payload_cannot_change_the_following_extent() -> Result<()> {
        let mut image = tempfile::tempfile()?;
        image.write_all(b"before----after")?;
        let result = copy_bounded(&mut Cursor::new(b"12345"), &mut image, 6, 4, false);
        assert!(result.is_err());
        image.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        image.read_to_end(&mut bytes)?;
        assert_eq!(bytes, b"before1234after");
        Ok(())
    }

    #[test]
    fn invalid_extent_is_rejected_before_writing() -> Result<()> {
        let mut image = tempfile::tempfile()?;
        image.write_all(b"unchanged")?;
        assert!(copy_bounded(&mut Cursor::new(b"x"), &mut image, u64::MAX, 2, false).is_err());
        assert!(copy_bounded(&mut Cursor::new(b"x"), &mut image, 8, 2, false).is_err());
        image.rewind()?;
        let mut bytes = String::new();
        image.read_to_string(&mut bytes)?;
        assert_eq!(bytes, "unchanged");
        Ok(())
    }

    #[test]
    fn failed_construction_preserves_original_and_removes_temporary() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let image = directory.path().join("image");
        fs::write(&image, b"original")?;
        assert!(
            replace_image(&image, |temporary| {
                fs::write(temporary, b"partial")?;
                bail!("conversion failed")
            })
            .is_err()
        );
        assert_eq!(fs::read(&image)?, b"original");
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        replace_image(&image, |temporary| Ok(fs::write(temporary, b"complete")?))?;
        assert_eq!(fs::read(&image)?, b"complete");
        Ok(())
    }
}
