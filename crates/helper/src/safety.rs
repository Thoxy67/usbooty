//! Pre-flight validation of the job's target device and input paths.
//!
//! The helper runs as root on input it must not trust: any unprivileged
//! process that can trigger the polkit action can feed it an arbitrary
//! `device_path` JSON field. Before any destructive module runs, the path is
//! checked to be (a) a real whole-disk block device and (b) not a disk the
//! running system lives on. `O_EXCL` alone is not enough: it is a no-op on
//! regular files and does not stop an explicit `/dev/sda` aimed at the
//! system disk. Fail closed: any doubt is an error.

use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::path::Path;

use usbooty_core::Job;

/// Mountpoints whose backing disks must never become a write target. The
/// removable-media locations (`/run/media`, `/media`, `/mnt`) are
/// deliberately absent; that is where a stick legitimately shows up.
const PROTECTED_MOUNTPOINTS: &[&str] = &[
    "/", "/boot", "/boot/efi", "/efi", "/usr", "/var", "/etc", "/home", "/srv", "/opt",
];

/// Validate that `device` is a whole-disk block device that is safe to
/// write: an absolute path resolving to a block device with a `/sys/block`
/// entry, whose disk does not back any protected mountpoint or active swap.
pub fn validate_target_device(device: &Path) -> Result<()> {
    if !device.is_absolute() {
        bail!(
            "device path {} is not absolute; refusing it",
            device.display()
        );
    }
    let meta = std::fs::metadata(device)
        .with_context(|| format!("checking the target device {}", device.display()))?;
    if !is_block_device(&meta) {
        bail!(
            "{} is not a block device; refusing to write to it",
            device.display()
        );
    }
    let name = block_name_of(rdev_of(&meta)).with_context(|| {
        format!(
            "could not resolve {} to a kernel block device name",
            device.display()
        )
    })?;
    if !Path::new(&format!("/sys/block/{name}")).exists() {
        bail!(
            "{} ({name}) is a partition, not a whole disk; refusing to write to it",
            device.display()
        );
    }
    if protected_disks().contains(&name) {
        bail!(
            "{} ({name}) backs the running system (one of {}, or active swap); \
             refusing to write to it",
            device.display(),
            PROTECTED_MOUNTPOINTS.join(" "),
        );
    }
    Ok(())
}

/// Require every auxiliary file path in the job to be absolute, so a
/// relative name starting with `-` can never reach a shelled-out tool's
/// argv (mcopy, mformat, mount, mkfs.*) looking like an option flag.
pub fn validate_job_paths(job: &Job) -> Result<()> {
    let check = |label: &str, path: &Path| -> Result<()> {
        if path.is_absolute() {
            Ok(())
        } else {
            bail!("{label} path {} is not absolute; refusing it", path.display())
        }
    };
    match job {
        Job::Dd { iso_path, .. } => check("ISO", iso_path),
        Job::Partitioned {
            iso_path,
            uefi_ntfs_img,
            ..
        } => {
            check("ISO", iso_path)?;
            if let Some(img) = uefi_ntfs_img {
                check("UEFI:NTFS image", img)?;
            }
            Ok(())
        }
        Job::Ventoy { iso_path, .. } => {
            if let Some(iso) = iso_path {
                check("ISO", iso)?;
            }
            Ok(())
        }
        Job::Backup { image_path, .. } => check("backup image", image_path),
        Job::Freedos {
            kernel_sys,
            command_com,
            boot_bin,
            ..
        } => {
            check("KERNEL.SYS", kernel_sys)?;
            check("COMMAND.COM", command_com)?;
            check("FreeDOS boot sector", boot_bin)
        }
        Job::Format { .. } | Job::Check { .. } => Ok(()),
    }
}

/// Whether `meta` describes a block device node.
fn is_block_device(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_block_device()
}

/// The `st_rdev` of a device node.
fn rdev_of(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.rdev()
}

/// Resolve a block device number to its kernel name (`sdb`, `nvme0n1`, ...)
/// via `/sys/dev/block/<major>:<minor>`. Also follows symlinked `/dev`
/// paths (e.g. `/dev/disk/by-id/...`) to the real node name.
fn block_name_of(rdev: u64) -> Option<String> {
    let major = nix::sys::stat::major(rdev);
    let minor = nix::sys::stat::minor(rdev);
    let real = std::fs::canonicalize(format!("/sys/dev/block/{major}:{minor}")).ok()?;
    Some(real.file_name()?.to_string_lossy().into_owned())
}

/// The set of whole-disk kernel names backing the protected mountpoints and
/// any active swap. Best-effort by necessity (a mount table can name things
/// sysfs cannot resolve), but every resolvable system disk lands in the set.
fn protected_disks() -> HashSet<String> {
    let mut out = HashSet::new();

    // st_dev of each protected mountpoint resolves directly for ordinary
    // filesystems (ext4, xfs, vfat, ...).
    for mp in PROTECTED_MOUNTPOINTS {
        if let Ok(meta) = std::fs::metadata(mp) {
            use std::os::unix::fs::MetadataExt;
            if let Some(name) = block_name_of(meta.dev()) {
                underlying_disks(&name, 0, &mut out);
            }
        }
    }

    // btrfs (and other multi-device filesystems) report anonymous st_dev
    // values; cover those through the mount table's source field. The
    // protected mountpoints contain no characters /proc/mounts escapes.
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let mut fields = line.split_whitespace();
            let (Some(src), Some(mp)) = (fields.next(), fields.next()) else {
                continue;
            };
            if !src.starts_with("/dev/") || !PROTECTED_MOUNTPOINTS.contains(&mp) {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(src)
                && is_block_device(&meta)
                && let Some(name) = block_name_of(rdev_of(&meta))
            {
                underlying_disks(&name, 0, &mut out);
            }
        }
    }

    // Disks holding active swap: overwriting one corrupts live memory pages.
    if let Ok(swaps) = std::fs::read_to_string("/proc/swaps") {
        for line in swaps.lines().skip(1) {
            let Some(src) = line.split_whitespace().next() else {
                continue;
            };
            if !src.starts_with("/dev/") {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(src)
                && is_block_device(&meta)
                && let Some(name) = block_name_of(rdev_of(&meta))
            {
                underlying_disks(&name, 0, &mut out);
            }
        }
    }

    out
}

/// Resolve `name` down to the whole-disk device(s) underneath it: dm-crypt /
/// LVM / md stacks recurse through `slaves/`, a partition maps to its parent
/// disk (the partition's sysfs dir nests inside the disk's), and a plain
/// disk maps to itself.
fn underlying_disks(name: &str, depth: u8, out: &mut HashSet<String>) {
    if depth > 8 {
        return; // cycle guard; real stacks are 2-3 levels deep
    }
    let mut had_slaves = false;
    if let Ok(entries) = std::fs::read_dir(format!("/sys/class/block/{name}/slaves")) {
        for entry in entries.flatten() {
            had_slaves = true;
            underlying_disks(&entry.file_name().to_string_lossy(), depth + 1, out);
        }
    }
    if had_slaves {
        return;
    }
    if Path::new(&format!("/sys/class/block/{name}/partition")).exists()
        && let Ok(real) = std::fs::canonicalize(format!("/sys/class/block/{name}"))
        && let Some(parent) = real.parent().and_then(|p| p.file_name())
    {
        out.insert(parent.to_string_lossy().into_owned());
        return;
    }
    out.insert(name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_files_are_rejected() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let err = validate_target_device(f.path()).unwrap_err();
        assert!(format!("{err:#}").contains("not a block device"));
    }

    #[test]
    fn missing_paths_are_rejected() {
        assert!(validate_target_device(Path::new("/no/such/usbooty-device")).is_err());
    }

    #[test]
    fn relative_paths_are_rejected() {
        let err = validate_target_device(Path::new("-evil")).unwrap_err();
        assert!(format!("{err:#}").contains("not absolute"));
    }

    #[test]
    fn relative_job_paths_are_rejected() {
        let job = Job::Dd {
            iso_path: "-not-absolute.iso".into(),
            device_path: "/dev/null".into(),
            opts: Default::default(),
        };
        assert!(validate_job_paths(&job).is_err());
        let job = Job::Dd {
            iso_path: "/tmp/fine.iso".into(),
            device_path: "/dev/null".into(),
            opts: Default::default(),
        };
        assert!(validate_job_paths(&job).is_ok());
    }

    #[test]
    fn the_root_disk_is_protected() {
        // Whatever disk(s) back `/` must be in the protected set on any
        // normally-booted system. Skip quietly on exotic CI roots (tmpfs,
        // network) where no block device resolves at all.
        let protected = protected_disks();
        let root_resolvable = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata("/")
                .ok()
                .and_then(|m| block_name_of(m.dev()))
                .is_some()
                || std::fs::read_to_string("/proc/mounts")
                    .unwrap_or_default()
                    .lines()
                    .any(|l| {
                        let mut f = l.split_whitespace();
                        f.next().is_some_and(|s| s.starts_with("/dev/"))
                            && f.next() == Some("/")
                    })
        };
        if root_resolvable {
            assert!(
                !protected.is_empty(),
                "the disk backing / must be detected as protected"
            );
        }
    }
}
