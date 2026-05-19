//! Filesystem helpers: waiting for partition device nodes, creating
//! filesystems, and mounting them at a private temporary mountpoint.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::emit;

/// Wait until `path` appears as a block device after a partition-table change.
/// `udev` creates these nodes asynchronously, so a short poll is required.
pub fn wait_for_node(path: &str) -> Result<()> {
    // Nudge udev along; ignore failure (the poll below is the real guarantee).
    let _ = Command::new("udevadm").arg("settle").status();

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if Path::new(path).exists() {
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

/// Run an external formatting tool, surfacing its output on failure.
fn run_tool(tool: &str, args: &[&str], doing: &str) -> Result<()> {
    emit::log(format!("Running {tool} {}", args.join(" ")));
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("could not run {tool} — is it installed?"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{doing} failed: {}", stderr.trim());
    }
    Ok(())
}

/// An RAII mount: mounts on construction, unmounts and cleans up on drop.
pub struct Mount {
    mountpoint: PathBuf,
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

        emit::log(format!("Mounted {device} ({fstype})"));
        Ok(Mount { mountpoint })
    }

    /// Mount an NTFS volume, preferring the in-kernel `ntfs3` driver and
    /// falling back to the older `ntfs` filesystem name.
    pub fn new_ntfs(device: &str) -> Result<Self> {
        match Self::new(device, "ntfs3") {
            Ok(mount) => Ok(mount),
            Err(_) => Self::new(device, "ntfs"),
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
    }
}
