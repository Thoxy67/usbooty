//! Filesystem helpers: waiting for partition device nodes, creating
//! filesystems, and mounting them at a private temporary mountpoint.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use usbooty_core::FileSystem;

use crate::emit;

/// Wait until `path` appears as a block device after a partition-table change.
/// `udev` creates these nodes asynchronously, so a short poll is required.
pub fn wait_for_node(path: &str) -> Result<()> {
    // Nudge udev along; ignore failure (the poll below is the real guarantee).
    let _ = Command::new("udevadm").arg("settle").status();

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if Path::new(path).exists() {
            emit::log(format!("Partition node {path} is ready"));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("partition device {path} did not appear within 10 s");
}

/// Create a FAT32 filesystem on `device`, labelled after the source image
/// (`label` is sanitized to FAT32's 11-character upper-case limit).
pub fn mkfs_vfat(device: &str, label: &str) -> Result<()> {
    let label = crate::label::fat(label);
    run_tool(
        "mkfs.vfat",
        &["-F", "32", "-n", label.as_str(), device],
        "creating the FAT32 filesystem",
    )
}

/// Create a FAT16 filesystem on `device`, labelled after the source image.
/// FAT16 caps volume size at 4 GiB; the helper relies on the GUI for that
/// check rather than re-implementing it here.
pub fn mkfs_vfat16(device: &str, label: &str) -> Result<()> {
    let label = crate::label::fat(label);
    run_tool(
        "mkfs.vfat",
        &["-F", "16", "-n", label.as_str(), device],
        "creating the FAT16 filesystem",
    )
}

/// Create an NTFS filesystem on `device` (quick format), labelled after the
/// source image (`label` is sanitized to NTFS's 32-character limit).
pub fn mkfs_ntfs(device: &str, label: &str) -> Result<()> {
    let label = crate::label::ntfs(label);
    run_tool(
        "mkfs.ntfs",
        &["--quick", "--force", "-L", label.as_str(), device],
        "creating the NTFS filesystem",
    )
}

/// Create an exFAT filesystem on `device`, labelled after the source image
/// (`label` is sanitized to exFAT's 15-character limit).
pub fn mkfs_exfat(device: &str, label: &str) -> Result<()> {
    let label = crate::label::exfat(label);
    run_tool(
        "mkfs.exfat",
        &["-L", label.as_str(), device],
        "creating the exFAT filesystem",
    )
}

/// Create an ext4 filesystem on `device`, labelled after the source image
/// (`label` is sanitized to ext4's 16-character limit).
pub fn mkfs_ext4(device: &str, label: &str) -> Result<()> {
    let label = crate::label::ext4(label);
    run_tool(
        "mkfs.ext4",
        &["-F", "-q", "-L", label.as_str(), device],
        "creating the ext4 filesystem",
    )
}

/// Create an ext3 filesystem on `device`.
pub fn mkfs_ext3(device: &str, label: &str) -> Result<()> {
    let label = crate::label::ext4(label);
    run_tool(
        "mkfs.ext3",
        &["-F", "-q", "-L", label.as_str(), device],
        "creating the ext3 filesystem",
    )
}

/// Create an ext2 filesystem on `device`.
pub fn mkfs_ext2(device: &str, label: &str) -> Result<()> {
    let label = crate::label::ext4(label);
    run_tool(
        "mkfs.ext2",
        &["-F", "-q", "-L", label.as_str(), device],
        "creating the ext2 filesystem",
    )
}

/// Create a UDF filesystem on `device` via `mkudffs` (udftools package).
///
/// `--media-type=hd` matches the fixed-disk emulation a USB stick presents
/// to firmware, which is what Linux/Windows/macOS expect when they probe a
/// thumb-drive partition.
pub fn mkfs_udf(device: &str, label: &str) -> Result<()> {
    let label = crate::label::udf(label);
    run_tool(
        "mkudffs",
        &["--media-type=hd", "--label", label.as_str(), device],
        "creating the UDF filesystem",
    )
}

/// Create a Btrfs filesystem on `device` via `mkfs.btrfs` (btrfs-progs).
pub fn mkfs_btrfs(device: &str, label: &str) -> Result<()> {
    let label = crate::label::btrfs(label);
    run_tool(
        "mkfs.btrfs",
        &["-f", "-L", label.as_str(), device],
        "creating the Btrfs filesystem",
    )
}

/// Create an XFS filesystem on `device` via `mkfs.xfs` (xfsprogs).
pub fn mkfs_xfs(device: &str, label: &str) -> Result<()> {
    let label = crate::label::xfs(label);
    run_tool(
        "mkfs.xfs",
        &["-f", "-L", label.as_str(), device],
        "creating the XFS filesystem",
    )
}

/// Create an F2FS filesystem on `device` via `mkfs.f2fs` (f2fs-tools).
pub fn mkfs_f2fs(device: &str, label: &str) -> Result<()> {
    let label = crate::label::f2fs(label);
    run_tool(
        "mkfs.f2fs",
        &["-f", "-l", label.as_str(), device],
        "creating the F2FS filesystem",
    )
}

/// Create a JFS filesystem on `device` via `mkfs.jfs` (jfsutils).
pub fn mkfs_jfs(device: &str, label: &str) -> Result<()> {
    let label = crate::label::jfs(label);
    run_tool(
        "mkfs.jfs",
        &["-q", "-L", label.as_str(), device],
        "creating the JFS filesystem",
    )
}

/// Create a NILFS2 filesystem on `device` via `mkfs.nilfs2` (nilfs-utils).
pub fn mkfs_nilfs2(device: &str, label: &str) -> Result<()> {
    let label = crate::label::nilfs2(label);
    run_tool(
        "mkfs.nilfs2",
        &["-f", "-L", label.as_str(), device],
        "creating the NILFS2 filesystem",
    )
}

/// Create `filesystem` on `device` with the given (raw) volume label.
pub fn mkfs(filesystem: FileSystem, device: &str, label: &str) -> Result<()> {
    match filesystem {
        FileSystem::Fat32 => mkfs_vfat(device, label),
        FileSystem::Fat16 => mkfs_vfat16(device, label),
        FileSystem::Ntfs => mkfs_ntfs(device, label),
        FileSystem::ExFat => mkfs_exfat(device, label),
        FileSystem::Ext4 => mkfs_ext4(device, label),
        FileSystem::Ext3 => mkfs_ext3(device, label),
        FileSystem::Ext2 => mkfs_ext2(device, label),
        FileSystem::Udf => mkfs_udf(device, label),
        FileSystem::Btrfs => mkfs_btrfs(device, label),
        FileSystem::Xfs => mkfs_xfs(device, label),
        FileSystem::F2fs => mkfs_f2fs(device, label),
        FileSystem::Jfs => mkfs_jfs(device, label),
        FileSystem::Nilfs2 => mkfs_nilfs2(device, label),
    }
}

/// Run an external formatting tool, surfacing its output in the log.
fn run_tool(tool: &str, args: &[&str], doing: &str) -> Result<()> {
    emit::log(format!("Running: {tool} {}", args.join(" ")));
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("could not run {tool} — is it installed?"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{doing} failed: {}", stderr.trim());
    }
    // Surface the tool's own output (version banners, warnings, …).
    for stream in [&output.stdout, &output.stderr] {
        for line in String::from_utf8_lossy(stream).lines() {
            let line = line.trim();
            if !line.is_empty() {
                emit::log(format!("  {line}"));
            }
        }
    }
    Ok(())
}

/// An RAII mount: mounts on construction, unmounts and cleans up on drop.
pub struct Mount {
    mountpoint: PathBuf,
    /// A human description of what is mounted, for the unmount log line.
    what: String,
}

impl Mount {
    /// Mount `device` (of filesystem type `fstype`) at a fresh private
    /// mountpoint under `/run`.
    pub fn new(device: &str, fstype: &str) -> Result<Self> {
        let mountpoint = PathBuf::from(format!("/run/usbooty-{}-{}", std::process::id(), fstype));
        std::fs::create_dir_all(&mountpoint)
            .with_context(|| format!("creating mountpoint {}", mountpoint.display()))?;

        let result = nix::mount::mount(
            Some(device),
            &mountpoint,
            Some(fstype),
            nix::mount::MsFlags::empty(),
            None::<&str>,
        );
        if let Err(e) = result {
            let _ = std::fs::remove_dir(&mountpoint);
            return Err(e).with_context(|| format!("mounting {device} ({fstype})"));
        }

        let what = format!("{device} ({fstype})");
        emit::log(format!("Mounted {what} at {}", mountpoint.display()));
        Ok(Mount { mountpoint, what })
    }

    /// Mount an NTFS volume, preferring the in-kernel `ntfs3` driver and
    /// falling back to the older `ntfs` filesystem name.
    pub fn new_ntfs(device: &str) -> Result<Self> {
        match Self::new(device, "ntfs3") {
            Ok(mount) => Ok(mount),
            Err(_) => Self::new(device, "ntfs"),
        }
    }

    /// Mount a partition holding `filesystem`, choosing the right driver.
    pub fn for_filesystem(device: &str, filesystem: FileSystem) -> Result<Self> {
        match filesystem {
            FileSystem::Fat32 | FileSystem::Fat16 => Self::new(device, "vfat"),
            FileSystem::Ntfs => Self::new_ntfs(device),
            FileSystem::ExFat => Self::new(device, "exfat"),
            FileSystem::Ext4 => Self::new(device, "ext4"),
            FileSystem::Ext3 => Self::new(device, "ext3"),
            FileSystem::Ext2 => Self::new(device, "ext2"),
            FileSystem::Udf => Self::new(device, "udf"),
            FileSystem::Btrfs => Self::new(device, "btrfs"),
            FileSystem::Xfs => Self::new(device, "xfs"),
            FileSystem::F2fs => Self::new(device, "f2fs"),
            FileSystem::Jfs => Self::new(device, "jfs"),
            FileSystem::Nilfs2 => Self::new(device, "nilfs2"),
        }
    }

    /// The directory where the filesystem is mounted.
    pub fn path(&self) -> &Path {
        &self.mountpoint
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        // Flush, then unmount; fall back to a lazy detach if the kernel is
        // still busy with writeback.
        nix::unistd::sync();
        if nix::mount::umount(&self.mountpoint).is_err() {
            let _ = nix::mount::umount2(&self.mountpoint, nix::mount::MntFlags::MNT_DETACH);
        }
        let _ = std::fs::remove_dir(&self.mountpoint);
        emit::log(format!("Unmounted {}", self.what));
    }
}
