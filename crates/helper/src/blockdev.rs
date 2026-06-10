//! Low-level block-device helpers: size queries, partition-table reread, and
//! unmounting whatever the kernel currently has mounted off the target device.

use anyhow::{Context, Result, bail};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::emit;

/// Zero-write chunk size for the buffered fallback path.
const ZERO_BUF: usize = 4 * 1024 * 1024;
/// Per-ioctl range for `BLKZEROOUT`, small enough that abort stays responsive
/// and progress keeps moving.
const ZEROOUT_CHUNK: u64 = 256 * 1024 * 1024;

// BLKGETSIZE64: `_IOR(0x12, 114, size_t)`, device size in bytes.
nix::ioctl_read!(blkgetsize64, 0x12, 114, u64);
// BLKRRPART: `_IO(0x12, 95)`, ask the kernel to re-read the partition table.
nix::ioctl_none!(blkrrpart, 0x12, 95);
// BLKFLSBUF: `_IO(0x12, 97)`, flush and invalidate the block-device page cache.
nix::ioctl_none!(blkflsbuf, 0x12, 97);
// BLKZEROOUT: `_IO(0x12, 127)`, zero a byte range `[start, len]` without
// transferring data from userspace (the kernel can use write-same/discard
// offload on devices that support it).
nix::ioctl_write_ptr_bad!(
    blkzeroout,
    nix::request_code_none!(0x12, 127),
    [u64; 2]
);

/// Open a whole-disk block device read/write with `O_EXCL`.
///
/// On a block device (Linux 2.6+), `O_EXCL` makes `open` fail with `EBUSY` if
/// the device, or any of its partitions, is mounted or otherwise claimed.
/// This closes the race between [`unmount_all`] and the first write: if the
/// target gets re-mounted (e.g. by a desktop automounter) in that window, the
/// job fails loudly here instead of silently corrupting an in-use disk.
pub fn open_exclusive(device: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::fcntl::OFlag::O_EXCL.bits())
        .open(device)
        .with_context(|| {
            format!(
                "opening {} exclusively; it may still be mounted or in use; \
                 close anything using the device and try again",
                device.display()
            )
        })
}

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

/// Flush and invalidate the kernel's page cache for an open block device.
///
/// Read-back verification is meaningless if the reads are served from the
/// pages the write pass just dirtied; `fsync` writes them back but does not
/// invalidate them. `BLKFLSBUF` does both, so the subsequent reads hit the
/// media. On a regular file (loopback-style test target) the ioctl returns
/// `ENOTTY`; that is fine, there is no fake hardware to catch there.
pub fn flush_page_cache(fd: &impl AsRawFd) -> Result<()> {
    // SAFETY: `fd` is a valid open descriptor.
    match unsafe { blkflsbuf(fd.as_raw_fd()) } {
        Ok(_) | Err(nix::errno::Errno::ENOTTY) => Ok(()),
        Err(e) => Err(e).context("BLKFLSBUF ioctl failed"),
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
pub fn partition_path(base: &Path, index: u32) -> String {
    let base = base.to_string_lossy();
    let needs_p = base.chars().next_back().is_some_and(|c| c.is_ascii_digit());
    if needs_p {
        format!("{base}p{index}")
    } else {
        format!("{base}{index}")
    }
}

/// Write zeros across the whole `device`: a "full format" erase that wipes
/// every stale filesystem and residual data before the new layout is written.
/// Reports an `Erasing` progress phase and honours `abort`.
pub fn zero_device(device: &Path, abort: &AtomicBool) -> Result<()> {
    let mut dev = open_exclusive(device)?;
    let size = device_size(&dev)?;

    emit::phase("Erasing");
    emit::log(format!(
        "Full format: erasing the whole device ({})",
        usbooty_core::device::format_size(size)
    ));

    // Prefer BLKZEROOUT: no userspace-to-kernel copy, no page-cache churn,
    // and devices with write-same/discard offload finish dramatically faster.
    // Regular files (test targets) and old kernels return ENOTTY/EOPNOTSUPP;
    // fall back to the buffered write loop there.
    let mut done = 0u64;
    let mut last = Instant::now();
    while done < size {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let chunk = (size - done).min(ZEROOUT_CHUNK);
        let range = [done, chunk];
        // SAFETY: `dev` is a valid open descriptor; `range` outlives the call.
        match unsafe { blkzeroout(dev.as_raw_fd(), &range) } {
            Ok(_) => {}
            Err(nix::errno::Errno::ENOTTY) | Err(nix::errno::Errno::EOPNOTSUPP) => {
                return zero_device_buffered(&mut dev, size, done, abort);
            }
            Err(e) => return Err(e).context("zeroing the target device (BLKZEROOUT)"),
        }
        done += chunk;
        if last.elapsed() >= Duration::from_millis(100) {
            emit::progress("Erasing", done, size);
            last = Instant::now();
        }
    }
    emit::progress("Erasing", size, size);
    nix::unistd::fsync(&dev).context("flushing the erased device")?;
    emit::log("Device fully erased");
    Ok(())
}

/// Buffered fallback for [`zero_device`]: write zeros through the page cache,
/// starting at `done` (bytes already zeroed via the ioctl path).
fn zero_device_buffered(
    dev: &mut File,
    size: u64,
    mut done: u64,
    abort: &AtomicBool,
) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    dev.seek(SeekFrom::Start(done))
        .context("seeking the target device")?;
    let buf = vec![0u8; ZERO_BUF];
    let mut last = Instant::now();
    while done < size {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let chunk = ((size - done) as usize).min(buf.len());
        dev.write_all(&buf[..chunk])
            .context("zeroing the target device")?;
        done += chunk as u64;
        if last.elapsed() >= Duration::from_millis(100) {
            emit::progress("Erasing", done, size);
            last = Instant::now();
        }
    }
    emit::progress("Erasing", size, size);
    dev.flush().ok();
    nix::unistd::fsync(&*dev).context("flushing the erased device")?;
    emit::log("Device fully erased");
    Ok(())
}

/// Decode the octal escapes `/proc/mounts` uses for space, tab, newline and
/// backslash in the source and mountpoint fields (`\040`, `\011`, `\012`,
/// `\134`). Without this, a stick automounted at e.g.
/// `/run/media/user/UBUNTU 2404` (our own FAT labels contain spaces) can
/// never be unmounted and the job aborts before it starts.
fn unescape_proc_mounts(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b))
        {
            let v = u32::from(bytes[i + 1] - b'0') * 64
                + u32::from(bytes[i + 2] - b'0') * 8
                + u32::from(bytes[i + 3] - b'0');
            // The kernel only escapes space/tab/newline/backslash, all ASCII;
            // leave anything else verbatim rather than mis-decode UTF-8.
            if v < 0x80 {
                out.push(v as u8);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Input was valid UTF-8 and we only ever substituted ASCII bytes.
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Unmount every filesystem currently mounted from `device` or any of its
/// partitions. Returns the number of filesystems unmounted.
pub fn unmount_all(device: &Path) -> Result<usize> {
    let device = device.to_string_lossy();
    let mounts = std::fs::read_to_string("/proc/mounts").context("reading /proc/mounts")?;

    // Collect targets first; unmounting deepest path first avoids EBUSY chains.
    let mut targets: Vec<String> = mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let src = unescape_proc_mounts(fields.next()?);
            let mountpoint = fields.next()?;
            // Match the whole disk or any of its partitions (`/dev/sdb`,
            // `/dev/sdb1`, ...). The trailing check avoids matching `/dev/sdbb`.
            let is_match = src == device
                || (src.starts_with(&*device)
                    && src[device.len()..]
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == 'p'));
            is_match.then(|| unescape_proc_mounts(mountpoint))
        })
        .collect();
    targets.sort_unstable_by_key(|m| std::cmp::Reverse(m.len()));

    let mut count = 0;
    for mountpoint in targets {
        let mountpoint = mountpoint.as_str();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_proc_mounts_decodes_octal() {
        assert_eq!(
            unescape_proc_mounts("/run/media/u/UBUNTU\\0402404"),
            "/run/media/u/UBUNTU 2404"
        );
        assert_eq!(unescape_proc_mounts("/a\\011b\\012c\\134d"), "/a\tb\nc\\d");
    }

    #[test]
    fn unescape_proc_mounts_passes_plain_paths_through() {
        assert_eq!(unescape_proc_mounts("/dev/sdb1"), "/dev/sdb1");
        // Lone or malformed backslash sequences stay verbatim.
        assert_eq!(unescape_proc_mounts("/a\\b"), "/a\\b");
        assert_eq!(unescape_proc_mounts("/a\\09"), "/a\\09");
        assert_eq!(unescape_proc_mounts("/trailing\\"), "/trailing\\");
    }
}
