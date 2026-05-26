//! The DD write method: a raw, byte-for-byte copy of the ISO onto the device,
//! transparently decompressing the source if it is gzip/xz/zstd/bzip2.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use usbooty_core::JobOptions;

use crate::{blockdev, emit, image};

/// Copy chunk size. 8 MiB is comfortable for modern USB 3+ devices; the
/// emit::progress throttle (0.1 % deduplication) keeps the GUI feed clean
/// regardless, so the coarser chunks roughly halve read/write syscall
/// count without any UI penalty.
const BUF_SIZE: usize = 8 * 1024 * 1024;
/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Write `iso_path` raw onto `device_path`, transparently decompressing it
/// when its extension says so.
pub fn run(
    iso_path: &Path,
    device_path: &Path,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    let mut img = image::open(iso_path)?;
    if img.compressed_size == 0 {
        bail!("the source image is empty");
    }
    if img.compressed {
        emit::log(format!(
            "Source: {} ({} bytes compressed, will be decompressed on the fly)",
            iso_path.display(),
            img.compressed_size
        ));
    } else {
        emit::log(format!(
            "Source: {} ({} bytes)",
            iso_path.display(),
            img.compressed_size
        ));
    }

    let unmounted = blockdev::unmount_all(device_path)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the target"
        ));
    }

    let mut dev = blockdev::open_exclusive(device_path)?;
    let dev_size = blockdev::device_size(&dev)?;
    // Compressed inputs have an unknown decompressed size, so the up-front
    // fit-check is skipped; a too-small device will fail cleanly on write.
    if !img.compressed && img.compressed_size > dev_size {
        bail!(
            "the image ({} bytes) is larger than the target device ({} bytes)",
            img.compressed_size,
            dev_size
        );
    }

    emit::phase("Writing");
    let mut buf = vec![0u8; BUF_SIZE];
    let mut decompressed: u64 = 0;
    let mut last = Instant::now();
    // Hash the decompressed stream as it passes by, for the optional verify.
    let mut src_hash = blake3::Hasher::new();
    loop {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let n = img.reader.read(&mut buf).context("reading from image")?;
        if n == 0 {
            break;
        }
        dev.write_all(&buf[..n])
            .context("writing to the target device")?;
        if opts.verify {
            src_hash.update(&buf[..n]);
        }
        decompressed += n as u64;
        if last.elapsed() >= REPORT_EVERY {
            // Progress tracks input-file bytes for a smooth, accurate bar
            // that does not depend on knowing the decompressed payload size.
            let done = img.bytes_read.load(Ordering::Relaxed);
            emit::progress("Writing", done, img.compressed_size);
            last = Instant::now();
        }
    }
    emit::progress("Writing", img.compressed_size, img.compressed_size);

    emit::phase("Flushing");
    emit::log("Flushing buffers to the device — this can take a while");
    dev.flush().context("flushing the target device")?;
    nix::unistd::fsync(&dev).context("fsync on the target device")?;

    if opts.verify {
        verify(device_path, decompressed, src_hash.finalize(), abort)?;
    }

    blockdev::reread_partition_table(&dev);
    if img.compressed {
        emit::log(format!(
            "Done — wrote {decompressed} decompressed bytes (from {} compressed)",
            img.compressed_size
        ));
    } else {
        emit::log(format!("Done — wrote {decompressed} bytes"));
    }
    Ok(())
}

/// Read the first `total` bytes back from `device` and confirm they hash to
/// `expected` — catching a silent bad write or failing flash.
fn verify(device: &Path, total: u64, expected: blake3::Hash, abort: &AtomicBool) -> Result<()> {
    emit::phase("Verifying");
    let mut dev =
        File::open(device).with_context(|| format!("reopening {} to verify", device.display()))?;
    dev.seek(SeekFrom::Start(0)).context("seeking the device")?;

    let mut hash = blake3::Hasher::new();
    let mut buf = vec![0u8; BUF_SIZE];
    let mut done = 0u64;
    let mut last = Instant::now();
    while done < total {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let want = ((total - done) as usize).min(buf.len());
        dev.read_exact(&mut buf[..want])
            .context("reading the device back")?;
        hash.update(&buf[..want]);
        done += want as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Verifying", done, total);
            last = Instant::now();
        }
    }
    emit::progress("Verifying", total, total);

    if hash.finalize() != expected {
        bail!("verification failed — the data read back does not match the ISO");
    }
    emit::log("Verification passed");
    Ok(())
}
