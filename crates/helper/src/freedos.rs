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

use crate::emit;

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
    mformat_boot_sector(&partition, boot_bin, filesystem, &opts.label)?;

    emit::phase("Copying FreeDOS files");
    mcopy_to_root(&partition, &[kernel_sys, command_com])?;

    // The MBR stub Syslinux ships will jump to the active partition's boot
    // sector, which now points at FreeDOS's bootloader code.
    crate::syslinux::write_mbr(device).context("writing the MBR stub")?;

    emit::log("FreeDOS bootable USB ready");
    Ok(())
}

/// Run `mformat -B <boot.bin> -i <partition> ::` to install the FreeDOS
/// boot sector. mformat re-formats the volume (new BPB, FATs and root
/// directory), choosing the FAT width by drive size when not told
/// otherwise, so the user's FAT16/FAT32 choice is passed explicitly with
/// `-F` (FAT32) and the volume label is re-applied with `-v` (the re-format
/// would otherwise discard what mkfs.vfat set). A boot sector whose FAT
/// width does not match the volume produces a stick that silently fails to
/// boot, so the result is verified afterwards.
fn mformat_boot_sector(
    partition: &str,
    boot_bin: &Path,
    filesystem: FileSystem,
    label: &str,
) -> Result<()> {
    let boot_bin_str = boot_bin.to_string_lossy();
    let label = crate::label::fat(label);
    let mut args = vec!["-B", &boot_bin_str, "-v", label.as_str()];
    if filesystem == FileSystem::Fat32 {
        args.push("-F");
    }
    args.extend_from_slice(&["-i", partition, "::"]);
    crate::fsutil::run_tool("mformat", &args, "installing the FreeDOS boot sector")?;
    verify_fat_width(partition, filesystem)
}

/// Confirm the boot sector mformat wrote matches the requested FAT width.
/// mformat has no flag forcing FAT16, so on a large partition it can lay
/// down FAT32 under a FAT16 boot sector; better to fail loudly here than
/// hand the user a non-booting stick.
fn verify_fat_width(partition: &str, filesystem: FileSystem) -> Result<()> {
    use std::io::Read;
    let mut sector = [0u8; 512];
    std::fs::File::open(partition)
        .and_then(|mut f| f.read_exact(&mut sector))
        .with_context(|| format!("reading the boot sector of {partition}"))?;
    // The informational FS-type string lives at offset 54 (FAT12/16) or 82
    // (FAT32); mformat fills it in.
    let is_fat32 = sector[82..90].starts_with(b"FAT32");
    let want_fat32 = filesystem == FileSystem::Fat32;
    if is_fat32 != want_fat32 {
        bail!(
            "mformat produced a {} volume but {} was requested; pick {} instead \
             (this partition size is outside what mtools supports for {})",
            if is_fat32 { "FAT32" } else { "FAT16" },
            filesystem.label(),
            if is_fat32 { "FAT32" } else { "FAT16" },
            filesystem.label(),
        );
    }
    Ok(())
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

    // mtools writes through the block device's page cache; flush it so the
    // boot files hit disk before we move on to writing the MBR. `sync()` is
    // global, no mount needed.
    nix::unistd::sync();
    Ok(())
}
