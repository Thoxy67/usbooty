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

/// Create `filesystem` on `device` with the given (raw) volume label.
pub fn mkfs(filesystem: FileSystem, device: &str, label: &str) -> Result<()> {
    match filesystem {
        FileSystem::Fat32 => mkfs_vfat(device, label),
        FileSystem::Ntfs => mkfs_ntfs(device, label),
        FileSystem::ExFat => mkfs_exfat(device, label),
        FileSystem::Ext4 => mkfs_ext4(device, label),
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
            FileSystem::Fat32 => Self::new(device, "vfat"),
            FileSystem::Ntfs => Self::new_ntfs(device),
            FileSystem::ExFat => Self::new(device, "exfat"),
            FileSystem::Ext4 => Self::new(device, "ext4"),
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
