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

use usbooty_core::{JobOptions, PartitionTable, WindowsSetup};

use crate::{blockdev, emit, fsutil, isocopy, partition};

/// Build the UEFI:NTFS layout on `device`.
pub fn run(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    uefi_ntfs_img: &Path,
    windows_setup: Option<&WindowsSetup>,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    let iso_size = std::fs::metadata(iso)
        .context("reading ISO metadata")?
        .len();
    // Defence in depth: re-validate the bootloader image here, in the
    // privileged helper, even though the GUI already checked it on download.
    let img_bytes = std::fs::read(uefi_ntfs_img).context("reading uefi-ntfs.img")?;
    if let Err(why) = usbooty_core::validate_uefi_ntfs(&img_bytes) {
        bail!("the uefi-ntfs.img is invalid: {why}");
    }
    let img_size = img_bytes.len() as u64;

    let unmounted = blockdev::unmount_all(device)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the target"
        ));
    }

    // A full format zeros the whole device before the new layout is written.
    if opts.full_format {
        blockdev::zero_device(device, abort)?;
    }

    emit::phase("Partitioning");
    {
        let mut dev = blockdev::open_exclusive(device)?;
        let dev_size = blockdev::device_size(&dev)?;
        if dev_size < iso_size + img_size {
            bail!("the target device is too small for the Windows files");
        }
        emit::log("Wiping old partition tables and filesystem signatures");
        partition::wipe_signatures(&mut dev, dev_size)?;
        emit::log(format!(
            "Writing the UEFI:NTFS layout ({table_label}): a large NTFS partition \
             plus a {} bootloader partition",
            usbooty_core::device::format_size(img_size),
            table_label = table.label(),
        ));
        partition::write_uefi_ntfs_layout(&mut dev, table, img_size, &opts.label)?;
        dev.flush().ok();
        let _ = nix::unistd::fsync(&dev);
        blockdev::reread_partition_table(&dev);
    }

    let ntfs_part = blockdev::partition_path(device, 1);
    let fat_part = blockdev::partition_path(device, 2);
    fsutil::wait_for_node(&ntfs_part)?;
    fsutil::wait_for_node(&fat_part)?;

    emit::phase("Formatting");
    fsutil::mkfs_ntfs(&ntfs_part, &opts.label)?;

    emit::phase("Copying");
    {
        let mount = fsutil::Mount::new_ntfs(&ntfs_part)?;
        isocopy::copy_iso(iso, mount.path(), abort, &|_| false)?;
        if opts.verify {
            isocopy::verify_iso(iso, mount.path(), abort, &|_| false)?;
        }
        if let Some(setup) = windows_setup {
            crate::unattend::write(mount.path(), setup)?;
        }
        emit::phase("Flushing");
        // `mount` drops here: sync + unmount.
    }

    emit::phase("Installing UEFI:NTFS bootloader");
    emit::log(format!(
        "Writing the UEFI:NTFS bootloader image to {fat_part}"
    ));
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
