//! Low-level block-device helpers: size queries, partition-table reread, and
//! unmounting whatever the kernel currently has mounted off the target device.

use anyhow::{Context, Result};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;

use crate::emit;

// BLKGETSIZE64: `_IOR(0x12, 114, size_t)` — device size in bytes.
nix::ioctl_read!(blkgetsize64, 0x12, 114, u64);
// BLKRRPART: `_IO(0x12, 95)` — ask the kernel to re-read the partition table.
nix::ioctl_none!(blkrrpart, 0x12, 95);

/// Total writable size of the target in bytes.
///
/// For a real block device this is the `BLKGETSIZE64` ioctl. When the target
/// is a regular file (a loopback-style image used for testing), the ioctl
/// returns `ENOTTY`; we then fall back to the file's length.
pub fn device_size(file: &File) -> Result<u64> {
    let mut size: u64 = 0;
    // SAFETY: `file` is a valid open descriptor; `size` is a valid pointer to a
    // u64 sized exactly as the ioctl expects.
    match unsafe { blkgetsize64(file.as_raw_fd(), &mut size) } {
        Ok(_) => Ok(size),
        Err(nix::errno::Errno::ENOTTY) => {
            Ok(file.metadata().context("stat of the target file")?.len())
        }
        Err(e) => Err(e).context("BLKGETSIZE64 ioctl failed"),
    }
}

/// Ask the kernel to re-read the partition table of an open device.
///
/// Best-effort: failures are logged but not fatal, since `udev`/`partprobe`
/// usually pick the change up regardless.
pub fn reread_partition_table(fd: &impl AsRawFd) {
    // SAFETY: `fd` is a valid open block-device descriptor.
    match unsafe { blkrrpart(fd.as_raw_fd()) } {
        Ok(_) => emit::log("Kernel re-read the partition table"),
        Err(e) => emit::warn(format!("Could not re-read partition table: {e}")),
    }
}

/// Derive the Nth partition's device node for a whole-disk device.
///
/// `nvme`/`mmcblk`/`loop` devices (whose name ends in a digit) use a `p`
/// separator; classic `sdX` devices do not.
// Used by the FAT32 method (milestone M3).
#[allow(dead_code)]
pub fn partition_path(base: &Path, index: u32) -> String {
    let base = base.to_string_lossy();
    let needs_p = base.chars().next_back().is_some_and(|c| c.is_ascii_digit());
    if needs_p {
        format!("{base}p{index}")
    } else {
        format!("{base}{index}")
    }
}

/// Unmount every filesystem currently mounted from `device` or any of its
/// partitions. Returns the number of filesystems unmounted.
pub fn unmount_all(device: &Path) -> Result<usize> {
    let device = device.to_string_lossy();
    let mounts = std::fs::read_to_string("/proc/mounts").context("reading /proc/mounts")?;

    // Collect targets first; unmounting deepest path last avoids EBUSY chains.
    let mut targets: Vec<&str> = mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let src = fields.next()?;
            let mountpoint = fields.next()?;
            // Match the whole disk or any of its partitions (`/dev/sdb`,
            // `/dev/sdb1`, ...). The trailing check avoids matching `/dev/sdbb`.
            let is_match = src == device
                || (src.starts_with(&*device)
                    && src[device.len()..]
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == 'p'));
            is_match.then_some(mountpoint)
        })
        .collect();
    targets.sort_unstable_by_key(|m| std::cmp::Reverse(m.len()));

    let mut count = 0;
    for mountpoint in targets {
        match nix::mount::umount(mountpoint) {
            Ok(()) => {
                emit::log(format!("Unmounted {mountpoint}"));
                count += 1;
            }
            Err(_) => {
                // Fall back to a lazy detach for stubborn mounts.
                nix::mount::umount2(mountpoint, nix::mount::MntFlags::MNT_DETACH)
                    .with_context(|| format!("unmounting {mountpoint}"))?;
                emit::log(format!("Lazily detached {mountpoint}"));
                count += 1;
            }
        }
    }
    Ok(count)
}
