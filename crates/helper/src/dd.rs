//! The DD write method: a raw, byte-for-byte copy of the ISO onto the device.

use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{blockdev, emit};

/// Copy chunk size. 4 MiB balances syscall overhead against memory use.
const BUF_SIZE: usize = 4 * 1024 * 1024;
/// Minimum interval between `Progress` messages, to avoid flooding the GUI.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Write `iso_path` raw onto `device_path`.
pub fn run(iso_path: &Path, device_path: &Path, abort: &AtomicBool) -> Result<()> {
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

    let mut dev = OpenOptions::new()
        .write(true)
        .open(device_path)
        .with_context(|| format!("opening device {}", device_path.display()))?;

    let dev_size = blockdev::device_size(&dev)?;
    if total > dev_size {
        bail!("the ISO ({total} bytes) is larger than the target device ({dev_size} bytes)");
    }

    emit::phase("Writing");
    let mut buf = vec![0u8; BUF_SIZE];
    let mut done: u64 = 0;
    let mut last = Instant::now();
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
        done += n as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Writing", done, total);
            last = Instant::now();
        }
    }
    emit::progress("Writing", done, total);

    emit::phase("Flushing");
    emit::log("Flushing buffers to the device — this can take a while");
    dev.flush().ok();
    nix::unistd::fsync(&dev).context("fsync on the target device")?;
    blockdev::reread_partition_table(&dev);

    emit::log(format!("Done — wrote {done} bytes"));
    Ok(())
}
