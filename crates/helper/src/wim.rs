//! Splitting a large `install.wim` into FAT32-friendly `install.swm` chunks.
//!
//! Used when the user picks the "split install.wim" option for a Windows ISO
//! whose install image exceeds the FAT32 4 GiB single-file limit.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use usbooty_core::PartitionTable;

use crate::{emit, fsutil, isocopy, partitioned};

/// Per-chunk size passed to `wimlib-imagex split`, in MiB. 4000 MiB keeps every
/// `install.swm` comfortably below the FAT32 4 GiB ceiling.
const SWM_CHUNK_MIB: &str = "4000";

/// Create one FAT32 partition, copy the ISO, and split `install.wim`.
pub fn run_split(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    label: &str,
    abort: &AtomicBool,
) -> Result<()> {
    if !tool_exists("wimlib-imagex") {
        bail!("wimlib-imagex is not installed (package: wimlib or wimtools)");
    }
    let iso_size = std::fs::metadata(iso)
        .context("reading ISO metadata")?
        .len();

    let part = partitioned::setup_single_fat32(device, table, iso_size, label)?;

    emit::phase("Copying");
    let mount = fsutil::Mount::new(&part, "vfat")?;
    // Copy everything except the oversized install image.
    isocopy::copy_iso(iso, mount.path(), abort, &|rel| {
        rel == "sources/install.wim" || rel == "sources/install.esd"
    })?;

    emit::phase("Splitting install.wim");
    let tmp = TempDir::new()?;
    let tmp_wim = tmp.path().join("install.wim");

    emit::log("Extracting install.wim from the ISO");
    isocopy::extract_file(iso, &["sources", "install.wim"], &tmp_wim).context(
        "extracting install.wim — note: install.esd cannot be split; use the UEFI:NTFS option",
    )?;

    let sources = mount.path().join("sources");
    std::fs::create_dir_all(&sources).ok();
    let swm = sources.join("install.swm");

    emit::log("Running wimlib-imagex split");
    let output = Command::new("wimlib-imagex")
        .arg("split")
        .arg(&tmp_wim)
        .arg(&swm)
        .arg(SWM_CHUNK_MIB)
        .output()
        .context("running wimlib-imagex")?;
    if !output.status.success() {
        bail!(
            "wimlib-imagex split failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    emit::phase("Flushing");
    drop(mount); // sync + unmount
    emit::log("install.wim split into install.swm chunks on a FAT32 partition");
    Ok(())
}

/// Whether `tool` is found on `PATH`.
fn tool_exists(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file()))
        .unwrap_or(false)
}

/// A temporary directory under `/var/tmp` (which has room for a multi-GiB
/// `install.wim`), removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self> {
        let path = PathBuf::from(format!("/var/tmp/usbooty-{}", std::process::id()));
        std::fs::create_dir_all(&path).context("creating temporary directory")?;
        Ok(TempDir(path))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
