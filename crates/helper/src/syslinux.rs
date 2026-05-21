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

use anyhow::{bail, Context, Result};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::process::Command;

use usbooty_core::FileSystem;

use crate::emit;

/// Distro-shipped locations of Syslinux's BIOS `mbr.bin` (the 440-byte master
/// boot record stub). The first one that exists wins.
const MBR_CANDIDATES: &[&str] = &[
    "/usr/lib/syslinux/bios/mbr.bin",
    "/usr/share/syslinux/mbr.bin",
    "/usr/lib/syslinux/mbr/mbr.bin",
    "/usr/lib/SYSLINUX/mbr.bin",
];

/// Install Syslinux on `partition` (the partition device node such as
/// `/dev/sdb1`), then stamp `mbr.bin` onto `device` (the parent disk).
/// `mount` is where `partition` is currently mounted (used for the ext4 path).
pub fn install(
    device: &Path,
    partition: &str,
    mount: &Path,
    filesystem: FileSystem,
) -> Result<()> {
    emit::phase("Installing Syslinux");

    // Step 1: lay down the syslinux config and `ldlinux.sys` in the new FS.
    match filesystem {
        FileSystem::Fat32 => install_fat(partition, mount)?,
        FileSystem::Ext4 => install_extlinux(mount)?,
        FileSystem::Ntfs | FileSystem::ExFat => {
            bail!("Syslinux installation is only supported on FAT32 or ext4");
        }
    }

    // Step 2: write the MBR boot sector onto the parent disk.
    write_mbr(device).context("writing the Syslinux MBR")
}

/// Run `syslinux --install` against a FAT partition node. The `--directory
/// /syslinux` option tells syslinux to look for `syslinux.cfg` in `/syslinux`
/// on the partition; if the ISO shipped an isolinux config under `/isolinux`,
/// we mirror it across so the boot menu actually loads.
fn install_fat(partition: &str, mount: &Path) -> Result<()> {
    ensure_syslinux_cfg(mount)?;
    let out = Command::new("syslinux")
        .args(["--install", "--directory", "/syslinux", partition])
        .output()
        .context("running syslinux")?;
    if !out.status.success() {
        bail!(
            "syslinux failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
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
    std::fs::create_dir_all(&target)
        .with_context(|| format!("creating {}", target.display()))?;

    // Mirror the isolinux config the same way as for FAT.
    let source_cfg = mount.join("isolinux").join("isolinux.cfg");
    let target_cfg = target.join("syslinux.cfg");
    if source_cfg.exists() && !target_cfg.exists() {
        std::fs::copy(&source_cfg, &target_cfg)
            .with_context(|| format!("copying {}", source_cfg.display()))?;
    }

    let out = Command::new("extlinux")
        .arg("--install")
        .arg(&target)
        .output()
        .context("running extlinux")?;
    if !out.status.success() {
        bail!(
            "extlinux failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    emit::log("Installed Extlinux to the ext4 partition");
    Ok(())
}

/// Write the Syslinux `mbr.bin` (a 440-byte boot-record stub) into the first
/// 440 bytes of `device`, preserving the rest of the existing MBR (partition
/// table + boot signature live at offset 440+ and must survive).
fn write_mbr(device: &Path) -> Result<()> {
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

    let mut dev = OpenOptions::new()
        .write(true)
        .open(device)
        .with_context(|| format!("opening {} to write the MBR", device.display()))?;
    dev.seek(SeekFrom::Start(0))
        .context("seeking to the start of the device")?;
    dev.write_all(&mbr[..440])
        .context("writing the Syslinux MBR stub")?;
    dev.flush().ok();
    nix::unistd::fsync(&dev).ok();
    emit::log(format!("Wrote Syslinux MBR stub from {mbr_path}"));
    Ok(())
}
