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
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{emit, fsutil};

/// BLAKE3 hashes of the copied source files, keyed by the lowercased,
/// `/`-joined relative path (the same key [`join_rel`] produces). Filled during
/// [`copy_iso`] and consumed by [`verify_iso`] so verification re-reads only the
/// destination instead of the source again.
pub type Hashes = HashMap<String, blake3::Hash>;

/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Files at least this large are named (with their size) in the log. Below it,
/// a file is only named when the caller asks to log every file.
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
    /// Name every copied file in the log, not just those past [`LOG_FILE_MIN`].
    log_all_files: bool,
    abort: &'a AtomicBool,
    /// Called with a lowercased relative path; `true` means "skip".
    skip: &'a dyn Fn(&str) -> bool,
    /// Copy pass: when set, hash each source file as it streams through `buf`
    /// (free, the bytes are already in memory) and record it in `src_hashes`.
    hash_source: bool,
    /// Copy pass: collected source hashes, keyed by relative path.
    src_hashes: Hashes,
    /// Verify pass: the source hashes from the copy, so we hash only the
    /// destination and compare. `None` (or a missing entry) falls back to
    /// re-hashing the source for that file.
    expected: Option<&'a Hashes>,
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
///
/// When `collect_hashes` is set, every copied file's source bytes are BLAKE3-
/// hashed as they stream through (no extra read), and the map of hashes is
/// returned so a following [`verify_iso`] only has to read the destination.
/// Pass the verify flag here; with it false an empty map is returned.
///
/// `log_all_files` names every copied file in the log; otherwise only files
/// past [`LOG_FILE_MIN`] are logged so a small-file flood is avoided.
pub fn copy_iso(
    iso_path: &Path,
    dest: &Path,
    abort: &AtomicBool,
    skip: &dyn Fn(&str) -> bool,
    collect_hashes: bool,
    log_all_files: bool,
) -> Result<Hashes> {
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
        log_all_files,
        abort,
        skip,
        hash_source: collect_hashes,
        src_hashes: Hashes::new(),
        expected: None,
    };
    copy_tree(iso.path(), dest, "", &mut ctx)?;
    ctx.report(true);
    emit::log(format!(
        "Copied {} file(s), {}",
        ctx.files,
        usbooty_core::device::format_size(ctx.copied)
    ));
    Ok(ctx.src_hashes)
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
                // squashfs, …) with their size. With log_all_files on, name
                // every file too (no size suffix on the small ones, so the
                // manifest stays readable); otherwise small files are silent
                // so they don't flood the log.
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if size >= LOG_FILE_MIN {
                    emit::log(format!(
                        "  {child_rel}  ({})",
                        usbooty_core::device::format_size(size)
                    ));
                } else if ctx.log_all_files {
                    emit::log(format!("  {child_rel}"));
                }
                copy_file(&src_path, &dest_path, &child_rel, ctx)?;
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
fn copy_file(src: &Path, dest: &Path, rel: &str, ctx: &mut Ctx) -> Result<()> {
    let mut reader = fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let mut writer =
        fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    // Hash the source as it streams through (verification will then only need
    // to re-read the destination, not the source again).
    let mut hasher = ctx.hash_source.then(blake3::Hasher::new);
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
        if let Some(h) = hasher.as_mut() {
            h.update(&ctx.buf[..n]);
        }
        writer
            .write_all(&ctx.buf[..n])
            .with_context(|| format!("writing {}", dest.display()))?;
        ctx.copied += n as u64;
        ctx.report(false);
    }
    if let Some(h) = hasher {
        ctx.src_hashes.insert(rel.to_string(), h.finalize());
    }
    ctx.files += 1;
    Ok(())
}

/// Re-read every copied file and confirm it matches the source ISO
/// byte-for-byte. Mirrors [`copy_iso`]; run after a copy when verification is
/// requested. The destination filesystem must still be mounted at `dest`.
/// `src_hashes` are the BLAKE3 hashes [`copy_iso`] computed for the source
/// files; verification hashes only the destination and compares against them,
/// halving the read load. Any file missing from the map falls back to
/// re-hashing the source, so correctness never depends on the cache.
pub fn verify_iso(
    iso_path: &Path,
    dest: &Path,
    abort: &AtomicBool,
    skip: &dyn Fn(&str) -> bool,
    src_hashes: &Hashes,
) -> Result<()> {
    let iso = fsutil::LoopMount::open_iso(iso_path, "src")?;
    let total = tree_size(iso.path(), "", skip)?;
    emit::phase("Verifying");
    // The copy just finished and the destination is still mounted, so its
    // pages are sitting dirty in the page cache; hashing now would verify RAM
    // against RAM. Write everything back first so the per-file
    // POSIX_FADV_DONTNEED in `hash_file` can actually evict the pages and the
    // reads hit the media.
    {
        let dest_dir =
            fs::File::open(dest).with_context(|| format!("opening {}", dest.display()))?;
        nix::unistd::syncfs(&dest_dir).context("syncing the destination before verify")?;
    }
    let mut ctx = Ctx {
        phase: "Verifying",
        copied: 0,
        files: 0,
        total,
        buf: vec![0u8; fsutil::COPY_BUF],
        last_report: Instant::now(),
        log_all_files: false,
        abort,
        skip,
        hash_source: false,
        src_hashes: Hashes::new(),
        expected: Some(src_hashes),
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
            // Prefer the hash recorded during the copy; only re-read the source
            // when it's absent (blake3::Hash is Copy, so take it before the
            // mutable borrow of ctx.buf below).
            let expected = ctx.expected.and_then(|m| m.get(&child_rel)).copied();
            let dest_hash = hash_file(&dest_path, &mut ctx.buf, ctx.abort, true)?;
            let src_hash = match expected {
                Some(h) => h,
                None => hash_file(&src_path, &mut ctx.buf, ctx.abort, false)?,
            };
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
///
/// With `drop_cache` set, the file's cached pages are evicted first (used for
/// destination files, which were just written: without the eviction the hash
/// would be computed over the page cache instead of the media).
fn hash_file(
    path: &Path,
    buf: &mut [u8],
    abort: &AtomicBool,
    drop_cache: bool,
) -> Result<blake3::Hash> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    if drop_cache {
        use std::os::fd::AsFd;
        // Best-effort: a filesystem that ignores the advice just verifies
        // from cache, which is no worse than before.
        let _ = nix::fcntl::posix_fadvise(
            file.as_fd(),
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
        );
    }
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

    fn ctx<'a>(
        abort: &'a AtomicBool,
        skip: &'a dyn Fn(&str) -> bool,
        hash_source: bool,
    ) -> Ctx<'a> {
        Ctx {
            phase: "Test",
            copied: 0,
            files: 0,
            total: 0,
            buf: vec![0u8; 64 * 1024],
            last_report: Instant::now(),
            log_all_files: false,
            abort,
            skip,
            hash_source,
            src_hashes: Hashes::new(),
            expected: None,
        }
    }

    #[test]
    fn copy_hashes_source_and_verify_compares_against_them() {
        let base = std::env::temp_dir().join(format!("usbooty-copyhash-{}", std::process::id()));
        let (src, dst) = (base.join("src"), base.join("dst"));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), b"hello world").unwrap();
        fs::write(src.join("sub/b.bin"), vec![7u8; 4096]).unwrap();

        let abort = AtomicBool::new(false);
        let noskip = |_: &str| false;

        // Copy with hashing on; the source hashes are collected.
        let mut c = ctx(&abort, &noskip, true);
        copy_tree(&src, &dst, "", &mut c).unwrap();
        let hashes = c.src_hashes;
        assert!(hashes.contains_key("a.txt"));
        assert!(hashes.contains_key("sub/b.bin"));

        // Verify against the recorded source hashes: a faithful copy passes.
        let mut v = ctx(&abort, &noskip, false);
        v.expected = Some(&hashes);
        verify_tree(&src, &dst, "", &mut v).expect("intact copy should verify");

        // Tamper the destination: verification must now fail.
        fs::write(dst.join("a.txt"), b"HELLO WORLD").unwrap();
        let mut v = ctx(&abort, &noskip, false);
        v.expected = Some(&hashes);
        assert!(
            verify_tree(&src, &dst, "", &mut v).is_err(),
            "tamper must be caught"
        );

        // Fallback: with no recorded hashes, verify re-reads the source. Restore
        // the file so the copy is faithful again, then verify with an empty map.
        fs::write(dst.join("a.txt"), b"hello world").unwrap();
        let empty = Hashes::new();
        let mut v = ctx(&abort, &noskip, false);
        v.expected = Some(&empty);
        verify_tree(&src, &dst, "", &mut v).expect("fallback (hash source) should verify");

        let _ = fs::remove_dir_all(&base);
    }
}
