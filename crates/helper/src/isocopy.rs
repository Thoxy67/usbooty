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
//! bar, and the GUI's speed/ETA readout, keep moving even mid-file.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{emit, fsutil};

/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Files at least this large are named individually in the log.
const LOG_FILE_MIN: u64 = 16 * 1024 * 1024;

/// Mutable state threaded through the recursive copy or verify walk.
struct Ctx<'a> {
    /// Progress phase name reported to the GUI (`Copying` / `Verifying`).
    phase: &'static str,
    /// Bytes processed so far.
    copied: u64,
    /// Files processed so far.
    files: u64,
    /// Total bytes that will be processed: the sum of every non-skipped file.
    total: u64,
    /// Reused I/O buffer.
    buf: Vec<u8>,
    last_report: Instant,
    abort: &'a AtomicBool,
    /// Called with a lowercased relative path; `true` means "skip".
    skip: &'a dyn Fn(&str) -> bool,
}

impl Ctx<'_> {
    fn report(&mut self, force: bool) {
        if force || self.last_report.elapsed() >= REPORT_EVERY {
            emit::progress(self.phase, self.copied, self.total.max(self.copied));
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
    let iso = fsutil::LoopMount::open_iso(iso_path, "src")?;

    // Pre-pass: sum the sizes of exactly the files we will copy, so the
    // progress bar reflects the real remaining work and reaches 100%.
    let total = tree_size(iso.path(), "", skip)?;

    emit::log(format!(
        "Copying ISO contents ({}) to the target",
        usbooty_core::device::format_size(total)
    ));
    let mut ctx = Ctx {
        phase: "Copying",
        copied: 0,
        files: 0,
        total,
        buf: vec![0u8; fsutil::COPY_BUF],
        last_report: Instant::now(),
        abort,
        skip,
    };
    copy_tree(iso.path(), dest, "", &mut ctx)?;
    ctx.report(true);
    emit::log(format!(
        "Copied {} file(s), {}",
        ctx.files,
        usbooty_core::device::format_size(ctx.copied)
    ));
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
            total += entry
                .metadata()
                .with_context(|| format!("stat of {}", entry.path().display()))?
                .len();
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
                // Name the genuinely large files (kernels, install.wim,
                // squashfs, …) in the log; small files would just flood it.
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if size >= LOG_FILE_MIN {
                    emit::log(format!(
                        "  {child_rel}  ({})",
                        usbooty_core::device::format_size(size)
                    ));
                }
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
    ctx.files += 1;
    Ok(())
}

/// Re-read every copied file and confirm it matches the source ISO
/// byte-for-byte. Mirrors [`copy_iso`]; run after a copy when verification is
/// requested. The destination filesystem must still be mounted at `dest`.
pub fn verify_iso(
    iso_path: &Path,
    dest: &Path,
    abort: &AtomicBool,
    skip: &dyn Fn(&str) -> bool,
) -> Result<()> {
    let iso = fsutil::LoopMount::open_iso(iso_path, "src")?;
    let total = tree_size(iso.path(), "", skip)?;
    emit::phase("Verifying");
    let mut ctx = Ctx {
        phase: "Verifying",
        copied: 0,
        files: 0,
        total,
        buf: vec![0u8; fsutil::COPY_BUF],
        last_report: Instant::now(),
        abort,
        skip,
    };
    verify_tree(iso.path(), dest, "", &mut ctx)?;
    ctx.report(true);
    Ok(())
}

/// Recursively compare one ISO directory against its copy under `dest`.
fn verify_tree(src: &Path, dest: &Path, rel: &str, ctx: &mut Ctx) -> Result<()> {
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
            verify_tree(&src_path, &dest_path, &child_rel, ctx)?;
        } else if file_type.is_file() && !(ctx.skip)(&child_rel) {
            let src_hash = hash_file(&src_path, &mut ctx.buf, ctx.abort)?;
            let dest_hash = hash_file(&dest_path, &mut ctx.buf, ctx.abort)?;
            if src_hash != dest_hash {
                bail!("verification failed: {child_rel} does not match the source ISO");
            }
            ctx.copied += entry.metadata().map(|m| m.len()).unwrap_or(0);
            ctx.report(false);
        }
    }
    Ok(())
}

/// BLAKE3-hash a file, streaming it in chunks through the reused `buf`.
fn hash_file(path: &Path, buf: &mut [u8], abort: &AtomicBool) -> Result<blake3::Hash> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    loop {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let n = file
            .read(buf)
            .with_context(|| format!("reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

/// Extract a single file from the ISO to `dest`. `path` is the segment list
/// from the ISO root, e.g. `["sources", "install.wim"]`. Each segment is
/// matched case-insensitively. Returns the number of bytes written.
// Used by the Windows To Go method (extracts install.wim before applying it).
#[allow(dead_code)]
pub fn extract_file(iso_path: &Path, path: &[&str], dest: &Path) -> Result<u64> {
    let iso = fsutil::LoopMount::open_iso(iso_path, "src")?;
    let src = fsutil::ci_path(iso.path(), path)?;

    let mut reader = fs::File::open(&src).with_context(|| format!("opening {}", src.display()))?;
    let mut writer =
        fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    io::copy(&mut reader, &mut writer).with_context(|| format!("extracting {}", src.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
