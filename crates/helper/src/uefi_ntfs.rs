//! The UEFI:NTFS two-partition layout for Windows ISOs with a large
//! `install.wim`.
//!
//! Layout: a large NTFS partition holding all the Windows files (with
//! `install.wim` intact), plus a tiny FAT32 partition at the end of the disk
//! carrying the Secure-Boot-signed UEFI:NTFS bootloader image. UEFI firmware
//! boots the tiny FAT32 partition, which loads an NTFS driver and chains to
//! the Windows installer on the NTFS partition.

use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::PartitionTable;

use crate::{blockdev, emit, fsutil, isocopy, partition};

/// Build the UEFI:NTFS layout on `device`.
pub fn run(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    uefi_ntfs_img: &Path,
    label: &str,
    abort: &AtomicBool,
) -> Result<()> {
    let iso_size = std::fs::metadata(iso)
        .context("reading ISO metadata")?
        .len();
    let img_size = std::fs::metadata(uefi_ntfs_img)
        .context("reading uefi-ntfs.img")?
        .len();
    if img_size == 0 {
        bail!("the downloaded uefi-ntfs.img is empty");
    }

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
        if dev_size < iso_size + img_size {
            bail!("the target device is too small for the Windows files");
        }
        partition::wipe_signatures(&mut dev, dev_size)?;
        partition::write_uefi_ntfs_layout(&mut dev, table, img_size, label)?;
        dev.flush().ok();
        let _ = nix::unistd::fsync(&dev);
        blockdev::reread_partition_table(&dev);
    }

    let ntfs_part = blockdev::partition_path(device, 1);
    let fat_part = blockdev::partition_path(device, 2);
    fsutil::wait_for_node(&ntfs_part)?;
    fsutil::wait_for_node(&fat_part)?;

    emit::phase("Formatting");
    fsutil::mkfs_ntfs(&ntfs_part, label)?;

    emit::phase("Copying");
    {
        let mount = fsutil::Mount::new_ntfs(&ntfs_part)?;
        isocopy::copy_iso(iso, mount.path(), abort, &|_| false)?;
        emit::phase("Flushing");
        // `mount` drops here: sync + unmount.
    }

    emit::phase("Installing UEFI:NTFS bootloader");
    write_raw_image(uefi_ntfs_img, &fat_part)?;

    emit::log("UEFI:NTFS layout created");
    Ok(())
}

/// Write `image` raw onto the block device `dest`.
fn write_raw_image(image: &Path, dest: &str) -> Result<()> {
    let mut src = File::open(image).context("opening uefi-ntfs.img")?;
    let mut out = OpenOptions::new()
        .write(true)
        .open(dest)
        .with_context(|| format!("opening {dest}"))?;
    std::io::copy(&mut src, &mut out).context("writing the UEFI:NTFS image")?;
    out.flush().ok();
    nix::unistd::fsync(&out).context("flushing the UEFI:NTFS partition")?;
    Ok(())
}
