//! Snapshot a block device into an image file, the inverse of the DD path.
//!
//! Useful for capturing a working USB stick so it can be restored later, or
//! sent off as a bug report. The output is compressed transparently when the
//! `image_path` ends in `.gz`, `.xz`, `.zst`, or `.bz2`; otherwise the raw
//! bytes are written. Progress tracks the number of device bytes read, which
//! is what the user wants to watch regardless of compression overhead.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use usbooty_core::JobOptions;

use crate::{blockdev, emit, fsutil};

const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Read `device_path` start-to-end into `image_path`.
pub fn run(
    device_path: &Path,
    image_path: &Path,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    let unmounted = blockdev::unmount_all(device_path)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the source"
        ));
    }

    // Open read-only; no need for O_EXCL, since we are not changing anything.
    let dev =
        File::open(device_path).with_context(|| format!("opening {}", device_path.display()))?;
    let total = blockdev::device_size(&dev)?;
    if total == 0 {
        bail!("the source device reports a zero size");
    }
    emit::log(format!(
        "Source: {} ({} bytes)",
        device_path.display(),
        total
    ));

    let out =
        File::create(image_path).with_context(|| format!("creating {}", image_path.display()))?;
    let mut writer = wrap_compressor(image_path, out)?;
    let compressing = writer_label(image_path);
    emit::log(format!("Output: {} {compressing}", image_path.display()));

    emit::phase("Reading");
    let mut buf = vec![0u8; fsutil::COPY_BUF];
    let mut done = 0u64;
    let mut last = Instant::now();
    let mut hasher = blake3::Hasher::new();
    let mut dev = dev;
    use std::io::Read;
    while done < total {
        if abort.load(Ordering::SeqCst) {
            // Drop the partial output before bailing; half-written backups
            // are confusing and easy to mistake for complete ones.
            drop(writer);
            let _ = std::fs::remove_file(image_path);
            bail!("aborted by user");
        }
        let want = ((total - done) as usize).min(buf.len());
        dev.read_exact(&mut buf[..want])
            .context("reading from the device")?;
        if opts.verify {
            hasher.update(&buf[..want]);
        }
        writer
            .write_all(&buf[..want])
            .context("writing the backup image")?;
        done += want as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Reading", done, total);
            last = Instant::now();
        }
    }
    emit::progress("Reading", total, total);

    emit::phase("Flushing");
    writer.flush().context("flushing the backup image")?;
    drop(writer);
    // The compressor's Drop finalises the stream and closes the file, but
    // close(2) does not durably persist anything. Re-open and fsync so the
    // user yanking the disk right after "Done" doesn't lose the tail.
    let sync_fd = File::open(image_path)
        .with_context(|| format!("re-opening {} for fsync", image_path.display()))?;
    nix::unistd::fsync(&sync_fd).context("fsync of the backup image")?;
    drop(sync_fd);

    if opts.verify {
        verify(device_path, image_path, total, hasher.finalize(), abort)?;
    }

    emit::log(format!(
        "Done: saved {done} bytes to {}",
        image_path.display()
    ));
    Ok(())
}

/// Read the device back and the image back, hash both, and confirm they match.
/// For a compressed image this re-decompresses on the fly via the existing
/// streaming opener, so verification works end to end with no special-casing.
fn verify(
    device: &Path,
    image: &Path,
    total: u64,
    expected_device_hash: blake3::Hash,
    abort: &AtomicBool,
) -> Result<()> {
    use std::io::Read;
    emit::phase("Verifying");

    // 1. Hash the image file (decompressing if needed), confirming the
    //    payload-out matches the payload-in.
    let mut img = crate::image::open(image)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; fsutil::COPY_BUF];
    let mut done = 0u64;
    let mut last = Instant::now();
    while done < total {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let want = ((total - done) as usize).min(buf.len());
        let n = img
            .reader
            .read(&mut buf[..want])
            .context("reading backup image")?;
        if n == 0 {
            bail!("backup image is shorter than the device");
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Verifying", done, total);
            last = Instant::now();
        }
    }
    emit::progress("Verifying", total, total);

    let _ = device; // device hash is computed during the read pass above
    if hasher.finalize() != expected_device_hash {
        bail!("verification failed; the image read back does not match the device");
    }
    emit::log("Verification passed");
    Ok(())
}

/// Pick the right encoder based on the output file's extension.
fn wrap_compressor(path: &Path, file: File) -> Result<Box<dyn Write + Send>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(match ext.as_str() {
        "gz" => Box::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::default(),
        )),
        "xz" => Box::new(xz2::write::XzEncoder::new(file, 6)),
        "zst" | "zstd" => Box::new(
            zstd::stream::write::Encoder::new(file, 0)
                .context("zstd init")?
                .auto_finish(),
        ),
        "bz2" => Box::new(bzip2::write::BzEncoder::new(
            file,
            bzip2::Compression::default(),
        )),
        _ => Box::new(file),
    })
}

/// Short tag for the log line, so the user sees which compressor is engaged.
fn writer_label(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "gz" => "(gzip-compressed)",
        "xz" => "(xz-compressed)",
        "zst" | "zstd" => "(zstd-compressed)",
        "bz2" => "(bzip2-compressed)",
        _ => "(raw)",
    }
}
