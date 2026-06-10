//! The UEFI:NTFS two-partition layout for Windows ISOs with a large
//! `install.wim`.
//!
//! Layout: a large NTFS partition holding all the Windows files (with
//! `install.wim` intact), plus a tiny FAT32 partition at the end of the disk
//! carrying the Secure-Boot-signed UEFI:NTFS bootloader image. UEFI firmware
//! boots the tiny FAT32 partition, which loads an NTFS driver and chains to
//! the Windows installer on the NTFS partition.

use anyhow::{Context, Result, bail};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::{FileSystem, JobOptions, PartitionTable, WindowsSetup};

use crate::{blockdev, emit, fsutil, isocopy, partition};

/// Build the UEFI:NTFS / UEFI:exFAT layout on `device`. The same bundled
/// bootloader image (`uefi-ntfs.img` from pbatard/uefi-ntfs) carries both
/// drivers, so the only filesystem-dependent steps are the mkfs and mount
/// calls.
/// Inputs to [`run`]. Grouped into one struct so the function signature
/// stays under the clippy `too_many_arguments` threshold.
pub struct UefiNtfsLayout<'a> {
    pub iso: &'a Path,
    pub device: &'a Path,
    pub table: PartitionTable,
    pub main_filesystem: FileSystem,
    pub uefi_ntfs_img: &'a Path,
    pub windows_setup: Option<&'a WindowsSetup>,
    pub opts: &'a JobOptions,
}

pub fn run(layout: UefiNtfsLayout<'_>, abort: &AtomicBool) -> Result<()> {
    let UefiNtfsLayout {
        iso,
        device,
        table,
        main_filesystem,
        uefi_ntfs_img,
        windows_setup,
        opts,
    } = layout;
    if !matches!(main_filesystem, FileSystem::Ntfs | FileSystem::ExFat) {
        bail!(
            "the UEFI:NTFS layout supports NTFS or exFAT for the main partition, not {}",
            main_filesystem.label()
        );
    }
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
        let sector_size = blockdev::logical_sector_size(&dev);
        if sector_size != 512 {
            emit::log(format!("Device reports {sector_size}-byte logical sectors"));
        }
        emit::log("Wiping old partition tables and filesystem signatures");
        partition::wipe_signatures(&mut dev, dev_size)?;
        emit::log(format!(
            "Writing the UEFI:NTFS layout ({table_label}): a large NTFS partition \
             plus a {} bootloader partition",
            usbooty_core::device::format_size(img_size),
            table_label = table.label(),
        ));
        partition::write_uefi_ntfs_layout(&mut dev, table, img_size, &opts.label, sector_size as u64)?;
        dev.flush().ok();
        let _ = nix::unistd::fsync(&dev);
        blockdev::reread_partition_table(&dev);
    }

    let main_part = blockdev::partition_path(device, 1);
    let fat_part = blockdev::partition_path(device, 2);
    fsutil::wait_for_node(&main_part)?;
    fsutil::wait_for_node(&fat_part)?;

    emit::phase("Formatting");
    fsutil::mkfs(main_filesystem, &main_part, &opts.label)?;

    emit::phase("Copying");
    {
        let mount = fsutil::Mount::for_filesystem(&main_part, main_filesystem)?;
        let src_hashes = isocopy::copy_iso(
            iso,
            mount.path(),
            abort,
            &|_| false,
            opts.verify,
            opts.log_all_files,
        )?;
        if opts.verify {
            isocopy::verify_iso(iso, mount.path(), abort, &|_| false, &src_hashes)?;
        }
        if let Some(setup) = windows_setup {
            crate::unattend::write(mount.path(), setup)?;
            // Optional Windows CA 2023 policy (`SkuSiPolicy.p7b`).
            if setup.windows_ca_2023 {
                crate::winca2023::apply(iso, mount.path())?;
            }
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
    let out = OpenOptions::new()
        .write(true)
        .open(dest)
        .with_context(|| format!("opening {dest}"))?;
    // Buffer device writes into COPY_BUF-sized chunks (matching dd / copy)
    // instead of io::copy's default 8 KiB, so the ~1 MiB image isn't dribbled
    // out in tiny writes.
    let mut out = std::io::BufWriter::with_capacity(crate::fsutil::COPY_BUF, out);
    std::io::copy(&mut src, &mut out).context("writing the UEFI:NTFS image")?;
    let out = out
        .into_inner()
        .map_err(|e| e.into_error())
        .context("flushing the UEFI:NTFS image")?;
    nix::unistd::fsync(&out).context("flushing the UEFI:NTFS partition")?;
    Ok(())
}
