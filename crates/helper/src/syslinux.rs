//! Install Syslinux (FAT) or Extlinux (ext4) as the BIOS boot loader on a
//! freshly-populated partition, and stamp Syslinux's master boot record onto
//! the parent device.
//!
//! When usbooty copies an isolinux-based Linux ISO file-by-file onto FAT32 or
//! ext4, the *contents* of `/isolinux` (kernel, initrd, config) land safely
//! but the BIOS boot sector and MBR do not, so the resulting drive will not
//! boot on legacy hardware. Re-running syslinux against the new partition and
//! writing the matching `mbr.bin` to the device's first 440 bytes fixes that
//! cleanly without disturbing the partition table that already lives there.

use anyhow::{Context, Result, bail};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use usbooty_core::FileSystem;

use crate::{blockdev, emit};

/// Distro-shipped locations of Syslinux's BIOS `mbr.bin` (the 440-byte master
/// boot record stub). The first one that exists wins.
const MBR_CANDIDATES: &[&str] = &[
    "/usr/lib/syslinux/bios/mbr.bin",
    "/usr/share/syslinux/mbr.bin",
    "/usr/lib/syslinux/mbr/mbr.bin",
    "/usr/lib/SYSLINUX/mbr.bin",
];

/// Lay down the Syslinux config and `ldlinux.sys` on the partition's
/// filesystem. Run from **inside** the mount RAII scope — the partition is
/// still mounted at `mount` and the bootloader needs that to install.
///
/// The MBR boot sector is intentionally **not** written here; call
/// [`write_mbr`] after the mount drops so it can take an exclusive
/// whole-disk lock.
pub fn install_files(partition: &str, mount: &Path, filesystem: FileSystem) -> Result<()> {
    emit::phase("Installing Syslinux");
    match filesystem {
        FileSystem::Fat32 | FileSystem::Fat16 => install_fat(partition, mount),
        FileSystem::Ext4 | FileSystem::Ext3 | FileSystem::Ext2 => install_extlinux(mount),
        FileSystem::Ntfs
        | FileSystem::ExFat
        | FileSystem::Udf
        | FileSystem::Btrfs
        | FileSystem::Xfs
        | FileSystem::F2fs
        | FileSystem::Jfs
        | FileSystem::Nilfs2 => {
            // Syslinux only ships boot blocks for FAT and ext2/3/4. Other
            // filesystems are bootable via GRUB or systemd-boot, but those
            // paths aren't wired through yet.
            bail!(
                "Syslinux installation is only supported on FAT12/16/32 or ext2/3/4 — \
                 {} cannot host the Syslinux boot files",
                filesystem.label()
            );
        }
    }
}

/// Run `syslinux --install` against a FAT partition node. The `--directory
/// /syslinux` option tells syslinux to look for `syslinux.cfg` in `/syslinux`
/// on the partition; if the ISO shipped an isolinux config under `/isolinux`,
/// we mirror it across so the boot menu actually loads.
fn install_fat(partition: &str, mount: &Path) -> Result<()> {
    ensure_syslinux_cfg(mount)?;
    crate::fsutil::run_tool(
        "syslinux",
        &["--install", "--directory", "/syslinux", partition],
        "installing Syslinux",
    )?;
    emit::log("Installed Syslinux to the FAT partition");
    Ok(())
}

/// Mirror `isolinux/isolinux.cfg` to `syslinux/syslinux.cfg` if the new
/// location does not exist yet — needed because the ISO's bootloader directory
/// is named `isolinux` but Syslinux on disk looks at `syslinux`.
fn ensure_syslinux_cfg(mount: &Path) -> Result<()> {
    let target_dir = mount.join("syslinux");
    let target = target_dir.join("syslinux.cfg");
    if target.exists() {
        return Ok(());
    }
    let source = mount.join("isolinux").join("isolinux.cfg");
    if !source.exists() {
        // No isolinux config to mirror — syslinux will still install and look
        // for `boot/syslinux.cfg` or fail gracefully at boot time.
        return Ok(());
    }
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;
    std::fs::copy(&source, &target)
        .with_context(|| format!("copying {} → {}", source.display(), target.display()))?;
    emit::log("Mirrored isolinux.cfg → syslinux/syslinux.cfg");
    Ok(())
}

/// Run `extlinux --install` against an ext4 mountpoint, with the config
/// living in `<mount>/syslinux/`.
fn install_extlinux(mount: &Path) -> Result<()> {
    let target = mount.join("syslinux");
    std::fs::create_dir_all(&target).with_context(|| format!("creating {}", target.display()))?;

    // Mirror the isolinux config the same way as for FAT.
    let source_cfg = mount.join("isolinux").join("isolinux.cfg");
    let target_cfg = target.join("syslinux.cfg");
    if source_cfg.exists() && !target_cfg.exists() {
        std::fs::copy(&source_cfg, &target_cfg)
            .with_context(|| format!("copying {}", source_cfg.display()))?;
    }

    let target_str = target.to_string_lossy();
    crate::fsutil::run_tool(
        "extlinux",
        &["--install", &target_str],
        "installing Extlinux",
    )?;
    emit::log("Installed Extlinux to the ext4 partition");
    Ok(())
}

/// Write the Syslinux `mbr.bin` (a 440-byte boot-record stub) into the first
/// 440 bytes of `device`, preserving the rest of the existing MBR (partition
/// table + boot signature live at offset 440+ and must survive).
///
/// Must be called **after** every partition on `device` is unmounted, so the
/// exclusive whole-disk open succeeds and the write isn't racing with the
/// kernel's partition rescan.
pub fn write_mbr(device: &Path) -> Result<()> {
    let mbr_path = MBR_CANDIDATES
        .iter()
        .find(|p| Path::new(p).is_file())
        .with_context(|| {
            "no Syslinux mbr.bin found in any of: ".to_string() + &MBR_CANDIDATES.join(", ")
        })?;
    let mbr = std::fs::read(mbr_path).with_context(|| format!("reading {mbr_path}"))?;
    if mbr.len() < 440 {
        bail!(
            "syslinux mbr.bin at {} is unexpectedly small ({} bytes)",
            mbr_path,
            mbr.len()
        );
    }

    let mut dev = blockdev::open_exclusive(device)
        .with_context(|| format!("opening {} exclusively to write the MBR", device.display()))?;
    dev.seek(SeekFrom::Start(0))
        .context("seeking to the start of the device")?;
    dev.write_all(&mbr[..440])
        .context("writing the Syslinux MBR stub")?;
    dev.flush().context("flushing the Syslinux MBR")?;
    nix::unistd::fsync(&dev).context("fsync of the Syslinux MBR")?;
    emit::log(format!("Wrote Syslinux MBR stub from {mbr_path}"));
    Ok(())
}
