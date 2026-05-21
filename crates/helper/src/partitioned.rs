//! The partition-and-copy method: partition the device, create a filesystem,
//! and copy the ISO contents onto it file by file.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::{
    FileSystem, JobOptions, PartitionTable, Persistence, WimStrategy, WindowsSetup,
};

use crate::{blockdev, emit, fsutil, isocopy, partition};

/// Run the partition-method job, dispatching on the large-`install.wim` strategy.
#[allow(clippy::too_many_arguments)]
pub fn run(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    wim: WimStrategy,
    uefi_ntfs_img: Option<&Path>,
    persistence: Option<Persistence>,
    windows_setup: Option<&WindowsSetup>,
    install_bootloader: bool,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    match wim {
        WimStrategy::None | WimStrategy::Split => plain_copy(
            iso,
            device,
            table,
            filesystem,
            persistence,
            windows_setup,
            install_bootloader,
            // Forward the split decision so plain_copy can replace
            // install.wim after the file-by-file copy lands it on FAT32.
            matches!(wim, WimStrategy::Split),
            opts,
            abort,
        ),
        WimStrategy::UefiNtfs => {
            let img = uefi_ntfs_img
                .context("the UEFI:NTFS layout requires the uefi-ntfs.img resource")?;
            crate::uefi_ntfs::run(iso, device, table, img, windows_setup, opts, abort)
        }
    }
}

/// The common case: one partition spanning the device, ISO contents copied in.
/// With `persistence`, a second ext4 partition is added for a writable overlay.
/// With `split_wim`, the copied `sources/install.wim` is post-processed into
/// FAT32-friendly `install.swm` chunks via `wimlib-imagex`.
#[allow(clippy::too_many_arguments)]
fn plain_copy(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    persistence: Option<Persistence>,
    windows_setup: Option<&WindowsSetup>,
    install_bootloader: bool,
    split_wim: bool,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    let iso_size = std::fs::metadata(iso)
        .context("reading ISO metadata")?
        .len();
    if iso_size == 0 {
        bail!("the source ISO is empty");
    }

    if let Some(p) = persistence {
        // Persistence is a Linux-only flow; wim splitting does not apply.
        let _ = split_wim;
        return copy_with_persistence(
            iso,
            device,
            table,
            filesystem,
            iso_size,
            p,
            install_bootloader,
            opts,
            abort,
        );
    }

    // When splitting install.wim, the file itself cannot land on FAT32 via
    // the normal file-by-file copy (the source is >4 GiB), so it is excluded
    // from the copy pass and `wimsplit` produces the chunked `install.swm`
    // files directly on the destination from the source ISO.
    let skip_install_wim: &dyn Fn(&str) -> bool =
        if split_wim { &|rel| rel == "sources/install.wim" } else { &|_| false };

    let part = setup_single_partition(device, table, filesystem, iso_size, opts, abort)?;
    emit::phase("Copying");
    {
        let mount = fsutil::Mount::for_filesystem(&part, filesystem)?;
        isocopy::copy_iso(iso, mount.path(), abort, skip_install_wim)?;
        if opts.verify {
            isocopy::verify_iso(iso, mount.path(), abort, skip_install_wim)?;
        }
        if split_wim {
            crate::wimsplit::split_install_wim(iso, mount.path(), abort)?;
        }
        if let Some(setup) = windows_setup {
            crate::unattend::write(mount.path(), setup)?;
        }
        if install_bootloader {
            crate::syslinux::install(device, &part, mount.path(), filesystem)?;
        }
        emit::phase("Flushing");
        // `mount` drops here: sync + unmount.
    }

    emit::log(format!(
        "{} partition created and populated",
        filesystem.label()
    ));
    Ok(())
}

/// Unmount the target, optionally erase it, write a single-partition table,
/// format it as `filesystem`, and return the partition's device node.
/// `min_size` is the minimum device size required (0 when there is no payload).
pub fn setup_single_partition(
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    min_size: u64,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<String> {
    prepare_device(device, opts, abort)?;

    emit::phase("Partitioning");
    {
        let mut dev = open_device(device)?;
        let dev_size = blockdev::device_size(&dev)?;
        if min_size > 0 && dev_size < min_size {
            bail!("the target device is smaller than the ISO contents");
        }
        emit::log(format!(
            "Target {} — {}",
            device.display(),
            usbooty_core::device::format_size(dev_size)
        ));
        emit::log("Wiping old partition tables and filesystem signatures");
        partition::wipe_signatures(&mut dev, dev_size)?;
        emit::log(format!(
            "Writing a {} partition table with one {} partition",
            table.label(),
            filesystem.label()
        ));
        partition::write_single_partition(&mut dev, table, filesystem, &opts.label)?;
        commit(&dev);
    }

    let part = blockdev::partition_path(device, 1);
    fsutil::wait_for_node(&part)?;

    emit::phase("Formatting");
    fsutil::mkfs(filesystem, &part, &opts.label)?;
    Ok(part)
}

/// The persistence variant: a main partition plus a trailing ext4 overlay.
#[allow(clippy::too_many_arguments)]
fn copy_with_persistence(
    iso: &Path,
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    iso_size: u64,
    persistence: Persistence,
    install_bootloader: bool,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    prepare_device(device, opts, abort)?;

    emit::phase("Partitioning");
    {
        let mut dev = open_device(device)?;
        let dev_size = blockdev::device_size(&dev)?;
        if dev_size < iso_size + persistence.size_bytes {
            bail!("the target device is too small for the ISO plus persistence");
        }
        emit::log("Wiping old partition tables and filesystem signatures");
        partition::wipe_signatures(&mut dev, dev_size)?;
        emit::log(format!(
            "Writing a {} table: a {} partition plus a {} ext4 persistence partition",
            table.label(),
            filesystem.label(),
            usbooty_core::device::format_size(persistence.size_bytes),
        ));
        partition::write_persistence_layout(
            &mut dev,
            table,
            filesystem,
            persistence.size_bytes,
            &opts.label,
        )?;
        commit(&dev);
    }

    let main_part = blockdev::partition_path(device, 1);
    let pers_part = blockdev::partition_path(device, 2);
    fsutil::wait_for_node(&main_part)?;
    fsutil::wait_for_node(&pers_part)?;

    emit::phase("Formatting");
    fsutil::mkfs(filesystem, &main_part, &opts.label)?;

    emit::phase("Copying");
    {
        let mount = fsutil::Mount::for_filesystem(&main_part, filesystem)?;
        isocopy::copy_iso(iso, mount.path(), abort, &|_| false)?;
        if opts.verify {
            isocopy::verify_iso(iso, mount.path(), abort, &|_| false)?;
        }
        // Add the persistence kernel option to the copied bootloader configs,
        // so the live system actually uses the overlay partition.
        crate::persistence::patch_boot_config(mount.path(), persistence.kind)?;
        if install_bootloader {
            crate::syslinux::install(device, &main_part, mount.path(), filesystem)?;
        }
    }

    crate::persistence::setup(&pers_part, persistence.kind)?;
    emit::phase("Flushing");
    emit::log(format!(
        "{} live USB created with a persistence partition",
        filesystem.label()
    ));
    Ok(())
}

/// Unmount the target and, if requested, fully erase it.
fn prepare_device(device: &Path, opts: &JobOptions, abort: &AtomicBool) -> Result<()> {
    let unmounted = blockdev::unmount_all(device)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the target"
        ));
    }
    if opts.full_format {
        blockdev::zero_device(device, abort)?;
    }
    Ok(())
}

/// Open the whole-disk device read/write, exclusively.
fn open_device(device: &Path) -> Result<std::fs::File> {
    blockdev::open_exclusive(device)
}

/// Flush a freshly-written partition table and ask the kernel to re-read it.
fn commit(dev: &std::fs::File) {
    let _ = nix::unistd::fsync(dev);
    blockdev::reread_partition_table(dev);
}
