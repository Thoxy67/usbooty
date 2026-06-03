//! Filesystem helpers: waiting for partition device nodes, creating
//! filesystems, and mounting them at a private temporary mountpoint.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use usbooty_core::FileSystem;

use crate::emit;

/// Default chunk size for streaming file / device copies.
///
/// 8 MiB is comfortable for modern USB 3+ devices and matches the read-back
/// buffer in `dd::verify` and `backup`, so paired read+write paths share the
/// same syscall cadence regardless of which job is running. The progress
/// throttle (every 100 ms) keeps the GUI feed smooth at this size.
pub const COPY_BUF: usize = 8 * 1024 * 1024;

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
    // `lazy_itable_init=1,lazy_journal_init=1` defers inode-table and journal
    // zeroing to the kernel after first mount. It is the modern e2fsprogs
    // default, but stating it explicitly keeps the "Persistence" phase fast on
    // large overlays regardless of the host's mke2fs.conf.
    run_tool(
        "mkfs.ext4",
        &[
            "-F",
            "-q",
            "-E",
            "lazy_itable_init=1,lazy_journal_init=1",
            "-L",
            label.as_str(),
            device,
        ],
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

/// Run an external short-lived tool, surfacing its output in the log.
///
/// Use this for any subprocess that completes in well under a second and
/// doesn't need cancellation midway (mkfs.*, mformat, mcopy, syslinux,
/// extlinux). Long-running cancellable children (wimlib-imagex split /
/// extract, ventoy, mount) need direct stdin/stdout control and roll
/// their own.
pub(crate) fn run_tool(tool: &str, args: &[&str], doing: &str) -> Result<()> {
    emit::log(format!("Running: {tool} {}", args.join(" ")));
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("could not run {tool}. Is it installed?"))?;
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

    /// Mount an NTFS volume. Tries the in-kernel drivers first (`ntfs3`, then
    /// the legacy read-only `ntfs`), and finally the FUSE `ntfs-3g` helper for
    /// kernels built without any in-kernel NTFS support (e.g. some CachyOS /
    /// custom kernels expose only FUSE). The `mount(2)` syscall cannot use a
    /// FUSE filesystem, so that last step must shell out to `ntfs-3g`.
    pub fn new_ntfs(device: &str) -> Result<Self> {
        if let Ok(mount) = Self::new(device, "ntfs3") {
            return Ok(mount);
        }
        if let Ok(mount) = Self::new(device, "ntfs") {
            return Ok(mount);
        }
        Self::new_fuse_ntfs(device)
    }

    /// Mount NTFS via the FUSE `ntfs-3g` helper. Used when the kernel has no
    /// in-kernel NTFS driver. Unmounting still goes through the `umount(2)`
    /// path in [`Drop`], which works for FUSE mounts owned by root.
    fn new_fuse_ntfs(device: &str) -> Result<Self> {
        let mountpoint = PathBuf::from(format!("/run/usbooty-{}-ntfs", std::process::id()));
        std::fs::create_dir_all(&mountpoint)
            .with_context(|| format!("creating mountpoint {}", mountpoint.display()))?;

        let output = Command::new("ntfs-3g")
            .arg(device)
            .arg(&mountpoint)
            .output()
            .with_context(|| {
                format!(
                    "running ntfs-3g for {device}; install the `ntfs-3g` package \
                     (this kernel has no in-kernel NTFS driver)"
                )
            })?;
        if !output.status.success() {
            let _ = std::fs::remove_dir(&mountpoint);
            bail!(
                "mounting {device} via ntfs-3g failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let what = format!("{device} (ntfs-3g)");
        emit::log(format!("Mounted {what} at {}", mountpoint.display()));
        Ok(Mount { mountpoint, what })
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

/// A read-only loopback mount of a source ISO, unmounted on drop.
///
/// Tries UDF first (modern Windows ISOs are UDF), then ISO9660 (Linux ISOs
/// and older media). Goes through `mount(8)` rather than the `mount(2)`
/// syscall directly so the kernel's loop-device allocation is handled for
/// us; modern util-linux sets `LO_FLAGS_AUTOCLEAR` so the loop device frees
/// itself when this struct's `Drop` releases the mount.
pub struct LoopMount {
    mountpoint: PathBuf,
    iso: PathBuf,
}

impl LoopMount {
    /// Loop-mount `iso` read-only. `tag` distinguishes concurrent mountpoints
    /// inside the same helper process so e.g. an isocopy mount and a
    /// wimsplit mount cannot collide.
    pub fn open_iso(iso: &Path, tag: &str) -> Result<Self> {
        let mountpoint = PathBuf::from(format!("/run/usbooty-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&mountpoint)
            .with_context(|| format!("creating mountpoint {}", mountpoint.display()))?;

        let mut last_err = String::new();
        for fstype in ["udf", "iso9660"] {
            let output = Command::new("mount")
                .args(["-t", fstype, "-o", "loop,ro"])
                .arg(iso)
                .arg(&mountpoint)
                .output()
                .context("running mount. Is util-linux installed?")?;
            if output.status.success() {
                emit::log(format!("Mounted source ISO ({fstype})"));
                return Ok(LoopMount {
                    mountpoint,
                    iso: iso.to_path_buf(),
                });
            }
            last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        let _ = std::fs::remove_dir(&mountpoint);
        bail!("could not mount the source ISO: {last_err}");
    }

    /// The directory where the ISO is mounted.
    pub fn path(&self) -> &Path {
        &self.mountpoint
    }
}

impl Drop for LoopMount {
    fn drop(&mut self) {
        // Loop-device auto-clear (set by mount(8) for `-o loop`) detaches the
        // backing file once the mount is released; fall back to a lazy detach
        // if writeback is still in flight.
        if nix::mount::umount(&self.mountpoint).is_err() {
            let _ = nix::mount::umount2(&self.mountpoint, nix::mount::MntFlags::MNT_DETACH);
        }
        let _ = std::fs::remove_dir(&self.mountpoint);
        emit::log(format!("Unmounted source ISO {}", self.iso.display()));
    }
}

/// True when the `wimlib-imagex` binary is on the helper's PATH.
///
/// Used to gate flows that need to read or split a Windows `install.wim`
/// (the `Split` strategy and the Windows CA 2023 policy drop-in).
pub fn wimlib_available() -> bool {
    Command::new("wimlib-imagex")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve `segments` under `root` for *writing*, matching each existing path
/// component case-insensitively and falling back to the literal segment for any
/// component that doesn't exist yet. Unlike [`ci_path`] this never fails.
///
/// It exists because the install media is copied verbatim from the ISO (whose
/// directories are typically lower-cased, e.g. `sources`, `efi`), and the
/// destination may be mounted through the *case-sensitive* in-kernel `ntfs3`
/// driver. Joining a hard-coded `Sources`/`EFI` there would create a *second*
/// directory colliding with the copied one; two NTFS entries differing only in
/// case make the directory index ambiguous to Windows, so the boot manager can
/// no longer resolve `\sources\boot.wim` and fails with 0xc000000f. Resolving
/// the real existing component first keeps the write in the original directory.
pub fn ci_join(root: &Path, segments: &[&str]) -> PathBuf {
    let mut current = root.to_path_buf();
    for seg in segments {
        let matched = std::fs::read_dir(&current).ok().and_then(|entries| {
            entries.flatten().find_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(seg)
                    .then(|| e.file_name())
            })
        });
        match matched {
            Some(name) => current.push(name),
            None => current.push(seg),
        }
    }
    current
}

/// Resolve a `/`-segment path under `root`, matching each segment ignoring
/// case (ISO9660 may upper-case names; UDF preserves them).
pub fn ci_path(root: &Path, segments: &[&str]) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for seg in segments {
        let mut found = None;
        for entry in
            std::fs::read_dir(&current).with_context(|| format!("reading {}", current.display()))?
        {
            let entry = entry.context("reading a directory entry")?;
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(seg)
            {
                found = Some(entry.path());
                break;
            }
        }
        current = found.with_context(|| format!("{seg} not found in {}", current.display()))?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_path_matches_segments_ignoring_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("Sources").join("Install.WIM");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, b"x").unwrap();

        let resolved = ci_path(dir.path(), &["sources", "install.wim"]).expect("ci_path");
        assert_eq!(resolved, nested);
    }

    #[test]
    fn ci_path_errors_when_segment_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = ci_path(dir.path(), &["sources", "install.wim"]).unwrap_err();
        assert!(format!("{err:#}").contains("not found"));
    }
}
