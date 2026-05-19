//! Copying the contents of a source ISO onto a destination filesystem.
//!
//! The ISO is loop-mounted **read-only** so the *kernel* parses its filesystem.
//! This is essential for modern Windows ISOs: they are UDF images carrying only
//! a near-empty ISO9660 stub, so a userspace ISO9660 reader sees almost no
//! files. Letting the kernel mount it transparently handles UDF, ISO9660,
//! Joliet and Rock Ridge alike.
//!
//! Progress is byte-accurate: a pre-pass sums the sizes of exactly the files
//! that will be copied, and large files are streamed in chunks so the progress
//! bar — and the GUI's speed/ETA readout — keep moving even mid-file.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::emit;

/// Copy chunk size. 4 MiB balances syscall overhead against memory use.
const COPY_BUF: usize = 4 * 1024 * 1024;
/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// A read-only loopback mount of a source ISO, unmounted on drop.
struct IsoMount {
    mountpoint: PathBuf,
}

impl IsoMount {
    /// Loop-mount `iso` read-only. UDF is tried first (modern Windows ISOs),
    /// then ISO9660 (Linux ISOs and older media).
    fn new(iso: &Path) -> Result<Self> {
        let mountpoint = PathBuf::from(format!("/run/usbooty-src-{}", std::process::id()));
        fs::create_dir_all(&mountpoint)
            .with_context(|| format!("creating mountpoint {}", mountpoint.display()))?;

        let mut last_err = String::new();
        for fstype in ["udf", "iso9660"] {
            let output = Command::new("mount")
                .args(["-t", fstype, "-o", "loop,ro"])
                .arg(iso)
                .arg(&mountpoint)
                .output()
                .context("running mount — is util-linux installed?")?;
            if output.status.success() {
                emit::log(format!("Mounted source ISO ({fstype})"));
                return Ok(IsoMount { mountpoint });
            }
            last_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        let _ = fs::remove_dir(&mountpoint);
        bail!("could not mount the source ISO: {last_err}");
    }

    /// The directory where the ISO is mounted.
    fn path(&self) -> &Path {
        &self.mountpoint
    }
}

impl Drop for IsoMount {
    fn drop(&mut self) {
        // `umount` of a loop mount also detaches the auto-allocated loop device.
        let _ = Command::new("umount").arg(&self.mountpoint).status();
        let _ = fs::remove_dir(&self.mountpoint);
    }
}

/// Mutable state threaded through the recursive copy.
struct Ctx<'a> {
    /// Bytes copied so far.
    copied: u64,
    /// Total bytes that will be copied — the sum of every non-skipped file.
    total: u64,
    /// Reused copy buffer.
    buf: Vec<u8>,
    last_report: Instant,
    abort: &'a AtomicBool,
    /// Called with a lowercased relative path; `true` means "do not copy".
    skip: &'a dyn Fn(&str) -> bool,
}

impl Ctx<'_> {
    fn report(&mut self, force: bool) {
        if force || self.last_report.elapsed() >= REPORT_EVERY {
            emit::progress("Copying", self.copied, self.total.max(self.copied));
            self.last_report = Instant::now();
        }
    }
}

/// Join a lowercased child name onto a `/`-separated relative path.
fn join_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_ascii_lowercase()
    } else {
        format!("{rel}/{}", name.to_ascii_lowercase())
    }
}

/// Copy every file from the ISO at `iso_path` into `dest`, except those for
/// which `skip` (called with the lowercased, `/`-separated relative path)
/// returns true.
pub fn copy_iso(
    iso_path: &Path,
    dest: &Path,
    abort: &AtomicBool,
    skip: &dyn Fn(&str) -> bool,
) -> Result<()> {
    let iso = IsoMount::new(iso_path)?;

    // Pre-pass: sum the sizes of exactly the files we will copy, so the
    // progress bar reflects the real remaining work and reaches 100%.
    let total = tree_size(iso.path(), "", skip)?;

    let mut ctx = Ctx {
        copied: 0,
        total,
        buf: vec![0u8; COPY_BUF],
        last_report: Instant::now(),
        abort,
        skip,
    };
    copy_tree(iso.path(), dest, "", &mut ctx)?;
    ctx.report(true);
    Ok(())
}

/// Sum the sizes of every file under `src` that will actually be copied
/// (i.e. for which `skip` returns false).
fn tree_size(src: &Path, rel: &str, skip: &dyn Fn(&str) -> bool) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry.context("reading an ISO directory entry")?;
        let name = entry.file_name();
        let child_rel = join_rel(rel, &name.to_string_lossy());
        let file_type = entry.file_type().context("stat of an ISO entry")?;
        if file_type.is_dir() {
            total += tree_size(&entry.path(), &child_rel, skip)?;
        } else if file_type.is_file() && !skip(&child_rel) {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(total)
}

/// Recursively copy one directory of the mounted ISO. `rel` is the lowercased
/// `/`-joined path from the ISO root, used for `skip` decisions.
fn copy_tree(src: &Path, dest: &Path, rel: &str, ctx: &mut Ctx) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;

    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        if ctx.abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let entry = entry.context("reading an ISO directory entry")?;
        let raw_name = entry.file_name();
        let name = raw_name.to_string_lossy();
        let child_rel = join_rel(rel, &name);
        let file_type = entry.file_type().context("stat of an ISO entry")?;
        let src_path = entry.path();
        let dest_path = dest.join(name.as_ref());

        if file_type.is_dir() {
            copy_tree(&src_path, &dest_path, &child_rel, ctx)?;
        } else if file_type.is_file() {
            if !(ctx.skip)(&child_rel) {
                copy_file(&src_path, &dest_path, ctx)?;
            }
        } else {
            // Rock Ridge symlinks: the destination (FAT/NTFS) has no symlinks,
            // and bootable Windows/Linux media never relies on them.
            emit::warn(format!("Skipping non-regular entry {child_rel}"));
        }
    }
    Ok(())
}

/// Copy one file in chunks, reporting progress (and so refreshing the GUI's
/// speed/ETA) even part-way through a multi-gigabyte file like `install.wim`.
fn copy_file(src: &Path, dest: &Path, ctx: &mut Ctx) -> Result<()> {
    let mut reader = fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let mut writer =
        fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    loop {
        if ctx.abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let n = reader
            .read(&mut ctx.buf)
            .with_context(|| format!("reading {}", src.display()))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&ctx.buf[..n])
            .with_context(|| format!("writing {}", dest.display()))?;
        ctx.copied += n as u64;
        ctx.report(false);
    }
    Ok(())
}

/// Extract a single file from the ISO to `dest`. `path` is the segment list
/// from the ISO root, e.g. `["sources", "install.wim"]`. Each segment is
/// matched case-insensitively. Returns the number of bytes written.
pub fn extract_file(iso_path: &Path, path: &[&str], dest: &Path) -> Result<u64> {
    let iso = IsoMount::new(iso_path)?;
    let src = resolve_ci(iso.path(), path)?;

    let mut reader = fs::File::open(&src).with_context(|| format!("opening {}", src.display()))?;
    let mut writer =
        fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    io::copy(&mut reader, &mut writer).with_context(|| format!("extracting {}", src.display()))
}

/// Resolve a `/`-segment path under `root`, matching each segment ignoring
/// case (ISO9660 may upper-case names; UDF preserves them).
fn resolve_ci(root: &Path, segments: &[&str]) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for segment in segments {
        let mut found = None;
        for entry in
            fs::read_dir(&current).with_context(|| format!("reading {}", current.display()))?
        {
            let entry = entry.context("reading an ISO directory entry")?;
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(segment)
            {
                found = Some(entry.path());
                break;
            }
        }
        current = found.with_context(|| format!("{segment} not found in the ISO"))?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_ci_matches_segments_ignoring_case() {
        let base = std::env::temp_dir().join(format!("usbooty-resolve-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("Sources")).unwrap();
        fs::write(base.join("Sources/Install.WIM"), b"x").unwrap();

        let hit = resolve_ci(&base, &["sources", "install.wim"]).unwrap();
        assert_eq!(hit, base.join("Sources/Install.WIM"));
        assert!(resolve_ci(&base, &["sources", "missing"]).is_err());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tree_size_sums_only_unskipped_files() {
        let base = std::env::temp_dir().join(format!("usbooty-treesize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sources")).unwrap();
        fs::write(base.join("readme.txt"), vec![0u8; 100]).unwrap();
        fs::write(base.join("sources/install.wim"), vec![0u8; 5000]).unwrap();

        let all = tree_size(&base, "", &|_| false).unwrap();
        assert_eq!(all, 5100);
        let no_wim = tree_size(&base, "", &|rel| rel == "sources/install.wim").unwrap();
        assert_eq!(no_wim, 100);

        let _ = fs::remove_dir_all(&base);
    }
}
