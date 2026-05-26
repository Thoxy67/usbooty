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

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use usbooty_core::{FileSystem, JobOptions, PartitionTable};

use crate::{emit, fsutil};

/// Build a FreeDOS bootable USB on `device` using the three already-cached
/// FreeDOS files. The single partition spans the device and is flagged
/// bootable on MBR layouts (FreeDOS doesn't care about GPT but the user
/// is free to pick either).
pub fn run(
    device: &Path,
    table: PartitionTable,
    filesystem: FileSystem,
    kernel_sys: &Path,
    command_com: &Path,
    boot_bin: &Path,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
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
    let partition = crate::partitioned::setup_single_partition(
        device, table, filesystem, 0, opts, abort,
    )?;

    emit::phase("Installing FreeDOS boot sector");
    mformat_boot_sector(&partition, boot_bin)?;

    emit::phase("Copying FreeDOS files");
    mcopy_to_root(&partition, &[kernel_sys, command_com])?;

    // The MBR stub Syslinux ships will jump to the active partition's boot
    // sector — which now points at FreeDOS's bootloader code.
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
    emit::log(format!(
        "Running: mformat -B {} -i {partition} ::",
        boot_bin.display()
    ));
    let out = Command::new("mformat")
        .arg("-B")
        .arg(boot_bin)
        .arg("-i")
        .arg(partition)
        .arg("::")
        .output()
        .context("running mformat — is `mtools` installed?")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "mformat failed installing the FreeDOS boot sector: {}",
            stderr.trim()
        );
    }
    Ok(())
}

/// Copy each `file` to the FAT root with `mcopy -i <partition> <file> ::`.
fn mcopy_to_root(partition: &str, files: &[&Path]) -> Result<()> {
    for file in files {
        emit::log(format!("mcopy -i {partition} {} ::", file.display()));
        let out = Command::new("mcopy")
            .arg("-i")
            .arg(partition)
            .arg("-Q")
            .arg("-o") // overwrite without prompting
            .arg(file)
            .arg("::")
            .output()
            .context("running mcopy — is `mtools` installed?")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "mcopy failed copying {} to the FAT root: {}",
                file.display(),
                stderr.trim()
            );
        }
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
