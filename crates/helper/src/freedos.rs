//! Lay down a FreeDOS bootable USB stick.
//!
//! No source ISO is involved. The GUI hands us paths to three already-
//! downloaded files (`KERNEL.SYS`, `COMMAND.COM`, and a `BOOT16.BIN` /
//! `BOOT32.BIN` boot sector matching the chosen FAT variant), and this
//! module assembles the boot stick:
//!
//!   1. Partition the device with one bootable FAT16 / FAT32 partition.
//!   2. Format it.
//!   3. Use `mformat -B` to overwrite the volume's boot sector with the
//!      FreeDOS one *while preserving the BPB* mkfs.vfat just wrote
//!      (the BPB depends on cluster size and partition geometry, so the
//!      raw boot-sector image cannot just be dd'd).
//!   4. `mcopy` `KERNEL.SYS` and `COMMAND.COM` to the FAT root.
//!   5. Stamp the existing Syslinux MBR onto the device so BIOSes chain
//!      to the partition's freshly-installed FreeDOS boot sector.
//!
//! Needs `mtools` (mformat, mcopy) and Syslinux's `mbr.bin` from the
//! existing dependency set. Both are advertised in `crates/gui/src/deps.rs`
//! and the AUR PKGBUILD.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use usbooty_core::{FileSystem, JobOptions, PartitionTable};

use crate::{emit, fsutil};

/// Inputs to [`run`]. Grouped into one struct so the function signature
/// stays under the clippy `too_many_arguments` threshold and so callers
/// can name each input at the call site.
pub struct FreedosLayout<'a> {
    pub device: &'a Path,
    pub table: PartitionTable,
    pub filesystem: FileSystem,
    pub kernel_sys: &'a Path,
    pub command_com: &'a Path,
    pub boot_bin: &'a Path,
    pub opts: &'a JobOptions,
}

/// Build a FreeDOS bootable USB using the three already-cached FreeDOS
/// files. The single partition spans the device and is flagged bootable on
/// MBR layouts (FreeDOS doesn't care about GPT but the user is free to
/// pick either).
pub fn run(layout: FreedosLayout<'_>, abort: &AtomicBool) -> Result<()> {
    let FreedosLayout {
        device,
        table,
        filesystem,
        kernel_sys,
        command_com,
        boot_bin,
        opts,
    } = layout;
    if !matches!(filesystem, FileSystem::Fat16 | FileSystem::Fat32) {
        bail!(
            "FreeDOS requires FAT16 or FAT32, not {}",
            filesystem.label()
        );
    }
    for (label, path) in [
        ("KERNEL.SYS", kernel_sys),
        ("COMMAND.COM", command_com),
        ("FreeDOS boot sector", boot_bin),
    ] {
        if !path.is_file() {
            bail!("{label} not found at {}", path.display());
        }
    }

    emit::phase("Partitioning");
    let partition =
        crate::partitioned::setup_single_partition(device, table, filesystem, 0, opts, abort)?;

    emit::phase("Installing FreeDOS boot sector");
    mformat_boot_sector(&partition, boot_bin)?;

    emit::phase("Copying FreeDOS files");
    mcopy_to_root(&partition, &[kernel_sys, command_com])?;

    // The MBR stub Syslinux ships will jump to the active partition's boot
    // sector, which now points at FreeDOS's bootloader code.
    crate::syslinux::write_mbr(device).context("writing the MBR stub")?;

    emit::log("FreeDOS bootable USB ready");
    Ok(())
}

/// Run `mformat -B <boot.bin> -i <partition> ::` to overwrite the FAT
/// volume's boot sector while preserving the BPB. mtools merges the BPB
/// from the partition with the boot code from `boot.bin`, which is the
/// only way to install a foreign boot sector onto a freshly-formatted
/// FAT volume without corrupting cluster-size / sector-count fields.
fn mformat_boot_sector(partition: &str, boot_bin: &Path) -> Result<()> {
    let boot_bin_str = boot_bin.to_string_lossy();
    crate::fsutil::run_tool(
        "mformat",
        &["-B", &boot_bin_str, "-i", partition, "::"],
        "installing the FreeDOS boot sector",
    )
}

/// Copy each `file` to the FAT root with `mcopy -i <partition> <file> ::`.
fn mcopy_to_root(partition: &str, files: &[&Path]) -> Result<()> {
    for file in files {
        let file_str = file.to_string_lossy();
        crate::fsutil::run_tool(
            "mcopy",
            &["-i", partition, "-Q", "-o", &file_str, "::"],
            "copying a file to the FreeDOS FAT root",
        )?;
    }

    // Keep the freshly-mounted-then-immediately-unmounted invariant the
    // rest of the helper uses: borrow the partition briefly to fsync the
    // FAT, so the boot files hit disk before we move on to writing the MBR.
    let mount = fsutil::Mount::for_filesystem(partition, FileSystem::Fat32).ok();
    if let Some(m) = mount {
        nix::unistd::sync();
        drop(m);
    }
    Ok(())
}
