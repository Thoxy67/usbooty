//! The FAT32 partition method: partition the device, create a filesystem, and
//! copy the ISO contents onto it file by file.

use anyhow::{bail, Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::{PartitionTable, WimStrategy};

use crate::{blockdev, emit, fsutil, isocopy, partition};

/// Run the partition-method job, dispatching on the large-`install.wim` strategy.
pub fn run(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    wim_strategy: WimStrategy,
    uefi_ntfs_img: Option<&Path>,
    label: &str,
    abort: &AtomicBool,
) -> Result<()> {
    match wim_strategy {
        WimStrategy::None => plain_fat32(iso, device, table, label, abort),
        WimStrategy::Split => crate::wim::run_split(iso, device, table, label, abort),
        WimStrategy::UefiNtfs => {
            let img = uefi_ntfs_img
                .context("the UEFI:NTFS layout requires the uefi-ntfs.img resource")?;
            crate::uefi_ntfs::run(iso, device, table, img, label, abort)
        }
    }
}

/// The common case: one FAT32 partition spanning the device, ISO copied in.
fn plain_fat32(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    label: &str,
    abort: &AtomicBool,
) -> Result<()> {
    let iso_size = std::fs::metadata(iso)
        .context("reading ISO metadata")?
        .len();
    if iso_size == 0 {
        bail!("the source ISO is empty");
    }

    let part = setup_single_fat32(device, table, iso_size, label)?;

    emit::phase("Copying");
    {
        let mount = fsutil::Mount::new(&part, "vfat")?;
        isocopy::copy_iso(iso, mount.path(), abort, &|_| false)?;
        emit::phase("Flushing");
        // `mount` drops here: sync + unmount.
    }

    emit::log("FAT32 partition created and populated");
    Ok(())
}

/// Unmount the target, write a single-FAT32-partition table, format it, and
/// return the partition's device node. Shared by the plain and WIM-split paths.
pub fn setup_single_fat32(
    device: &Path,
    table: PartitionTable,
    min_size: u64,
    label: &str,
) -> Result<String> {
    let unmounted = blockdev::unmount_all(device)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the target"
        ));
    }

    emit::phase("Partitioning");
    {
        let mut dev = OpenOptions::new()
            .read(true)
            .write(true)
            .open(device)
            .with_context(|| format!("opening device {}", device.display()))?;
        let dev_size = blockdev::device_size(&dev)?;
        if dev_size < min_size {
            bail!("the target device is smaller than the ISO contents");
        }
        partition::wipe_signatures(&mut dev, dev_size)?;
        partition::write_single_partition(&mut dev, table, label)?;
        dev.flush().ok();
        let _ = nix::unistd::fsync(&dev);
        blockdev::reread_partition_table(&dev);
    }

    let part = blockdev::partition_path(device, 1);
    fsutil::wait_for_node(&part)?;

    emit::phase("Formatting");
    fsutil::mkfs_vfat(&part, label)?;
    Ok(part)
}
