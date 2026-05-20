//! The DD write method: a raw, byte-for-byte copy of the ISO onto the device.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use usbooty_core::JobOptions;

use crate::{blockdev, emit};

/// Copy chunk size. 4 MiB balances syscall overhead against memory use.
const BUF_SIZE: usize = 4 * 1024 * 1024;
/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Write `iso_path` raw onto `device_path`.
pub fn run(
    iso_path: &Path,
    device_path: &Path,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    let mut iso =
        File::open(iso_path).with_context(|| format!("opening ISO {}", iso_path.display()))?;
    let total = iso.metadata().context("stat ISO")?.len();
    if total == 0 {
        bail!("the source ISO is empty");
    }
    emit::log(format!("Source: {} ({} bytes)", iso_path.display(), total));

    let unmounted = blockdev::unmount_all(device_path)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) from the target"
        ));
    }

    let mut dev = blockdev::open_exclusive(device_path)?;

    let dev_size = blockdev::device_size(&dev)?;
    if total > dev_size {
        bail!("the ISO ({total} bytes) is larger than the target device ({dev_size} bytes)");
    }

    emit::phase("Writing");
    let mut buf = vec![0u8; BUF_SIZE];
    let mut done: u64 = 0;
    let mut last = Instant::now();
    // Hash the source as it streams past, for the optional verify pass.
    let mut src_hash = blake3::Hasher::new();
    loop {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let n = iso.read(&mut buf).context("reading from ISO")?;
        if n == 0 {
            break;
        }
        dev.write_all(&buf[..n])
            .context("writing to the target device")?;
        if opts.verify {
            src_hash.update(&buf[..n]);
        }
        done += n as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Writing", done, total);
            last = Instant::now();
        }
    }
    emit::progress("Writing", done, total);

    emit::phase("Flushing");
    emit::log("Flushing buffers to the device — this can take a while");
    dev.flush().context("flushing the target device")?;
    nix::unistd::fsync(&dev).context("fsync on the target device")?;

    if opts.verify {
        verify(device_path, total, src_hash.finalize(), abort)?;
    }

    blockdev::reread_partition_table(&dev);
    emit::log(format!("Done — wrote {done} bytes"));
    Ok(())
}

/// Read the first `total` bytes back from `device` and confirm they hash to
/// `expected` — catching a silent bad write or failing flash.
fn verify(device: &Path, total: u64, expected: blake3::Hash, abort: &AtomicBool) -> Result<()> {
    emit::phase("Verifying");
    let mut dev = File::open(device)
        .with_context(|| format!("reopening {} to verify", device.display()))?;
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
