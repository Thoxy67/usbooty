//! Bad-blocks and counterfeit-drive detection.
//!
//! Counterfeit USB sticks are a frequent failure mode: they report a fake
//! capacity (often "256 GB" on a 32 GB chip), accept writes anywhere on the
//! device, but silently wrap or drop writes past the real boundary. A normal
//! ISO write succeeds and the user only finds out at boot, when files are
//! corrupted. The Quick mode here is the F3-style algorithm popularised by
//! `f3write`/`f3read`: write a unique fingerprint at sample positions and read
//! it back. The first mismatch is the effective capacity ceiling.
//!
//! Full mode is the classic destructive bad-blocks pass: write two patterns
//! across the whole device, read each back, and report any sectors that did
//! not return the expected bytes.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use usbooty_core::CheckMode;

use crate::{blockdev, emit};

const BLOCK: usize = 4096;
const REPORT_EVERY: Duration = Duration::from_millis(150);
/// Number of sample positions for the quick fake-drive test.
const QUICK_SAMPLES: u64 = 256;
/// Full-pass chunk size.
const FULL_BUF: usize = 4 * 1024 * 1024;

/// Final report produced by a device check, logged at the end of the run.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CheckReport {
    /// True when no failures were observed.
    pub ok: bool,
    /// File offsets (in bytes) of sectors that did not read back correctly.
    pub bad_offsets: Vec<u64>,
    /// When non-`None`, the drive lied about its capacity and this is the
    /// largest byte offset that survived a round-trip.
    pub effective_capacity: Option<u64>,
    /// Human-readable one-line summary, suitable for the status bar.
    pub summary: String,
}

/// Run the requested check on `device`, emitting progress and a final report.
pub fn run(device: &Path, mode: CheckMode, abort: &AtomicBool) -> Result<()> {
    let unmounted = blockdev::unmount_all(device)?;
    if unmounted > 0 {
        emit::log(format!(
            "Unmounted {unmounted} filesystem(s) before the check"
        ));
    }

    let report = match mode {
        CheckMode::Quick => quick(device, abort)?,
        CheckMode::Full => full(device, abort)?,
    };

    let json = serde_json::to_string(&report).context("serializing the check report")?;
    emit::log(format!("CHECK_REPORT {json}"));
    if !report.ok {
        bail!("{}", report.summary);
    }
    emit::log(report.summary);
    Ok(())
}

/// F3-style fake-drive check.
///
/// For each of `QUICK_SAMPLES` evenly-spaced positions, write a 4 KiB block
/// whose first 16 bytes encode `(seed, offset)`. After writing all of them,
/// read them back and check. The first mismatch (counting from the end of the
/// device) reveals the fake-capacity boundary.
fn quick(device: &Path, abort: &AtomicBool) -> Result<CheckReport> {
    let mut dev = blockdev::open_exclusive(device)?;
    let size = blockdev::device_size(&dev)?;
    if size < BLOCK as u64 * 2 {
        bail!("device too small for the quick check");
    }

    // Pick a per-run seed so a stale write from a previous run cannot
    // accidentally look like a fresh success.
    let seed: u64 = {
        let mut buf = [0u8; 8];
        let mut urandom = std::fs::File::open("/dev/urandom").context("opening urandom")?;
        urandom.read_exact(&mut buf).context("reading urandom")?;
        u64::from_le_bytes(buf)
    };

    let offsets = sample_offsets(size, QUICK_SAMPLES);

    emit::phase("Writing samples");
    let mut buf = vec![0u8; BLOCK];
    let mut last = Instant::now();
    for (i, &off) in offsets.iter().enumerate() {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        fill_fingerprint(&mut buf, seed, off);
        dev.seek(SeekFrom::Start(off))
            .context("seeking the device")?;
        dev.write_all(&buf)
            .with_context(|| format!("writing the sample at offset {off}"))?;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Writing samples", i as u64 + 1, offsets.len() as u64);
            last = Instant::now();
        }
    }
    dev.flush().context("flushing sample writes")?;
    nix::unistd::fsync(&dev).context("syncing sample writes to the device")?;
    // Drop the cached pages so the read-back hits the media; without this the
    // whole pass is served from RAM and a fake-capacity drive always "passes".
    blockdev::flush_page_cache(&dev)?;

    emit::phase("Reading samples back");
    let mut bad_offsets = Vec::new();
    let mut expected = vec![0u8; BLOCK];
    for (i, &off) in offsets.iter().enumerate() {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        fill_fingerprint(&mut expected, seed, off);
        dev.seek(SeekFrom::Start(off))
            .context("seeking the device")?;
        if dev.read_exact(&mut buf).is_err() {
            bad_offsets.push(off);
            continue;
        }
        if buf != expected {
            bad_offsets.push(off);
        }
        if last.elapsed() >= REPORT_EVERY {
            emit::progress("Reading samples back", i as u64 + 1, offsets.len() as u64);
            last = Instant::now();
        }
    }

    let ok = bad_offsets.is_empty();
    // The fake-capacity boundary: the highest offset that *did* survive.
    let effective_capacity = if ok {
        None
    } else {
        // O(1) membership instead of a linear scan per sample (the offset set
        // can be hundreds of entries on a heavily-faked drive).
        let bad: std::collections::HashSet<u64> = bad_offsets.iter().copied().collect();
        offsets.iter().copied().filter(|o| !bad.contains(o)).max()
    };
    let summary = if ok {
        format!(
            "Quick check passed: {} samples across {} of capacity matched",
            offsets.len(),
            usbooty_core::device::format_size(size)
        )
    } else {
        format!(
            "Quick check FAILED: {} of {} samples did not match{}",
            bad_offsets.len(),
            offsets.len(),
            match effective_capacity {
                Some(c) => format!(
                    "; likely fake-capacity drive (real ~{})",
                    usbooty_core::device::format_size(c)
                ),
                None => String::new(),
            }
        )
    };
    Ok(CheckReport {
        ok,
        bad_offsets,
        effective_capacity,
        summary,
    })
}

/// Destructive two-pattern bad-blocks scan. Returns offsets of any sectors
/// that did not survive both passes.
fn full(device: &Path, abort: &AtomicBool) -> Result<CheckReport> {
    let mut dev = blockdev::open_exclusive(device)?;
    let size = blockdev::device_size(&dev)?;

    let bad_a = scan_pattern(&mut dev, size, 0xAA, "Pattern 0xAA", abort)?;
    let bad_5 = scan_pattern(&mut dev, size, 0x55, "Pattern 0x55", abort)?;
    let mut bad: Vec<u64> = bad_a.into_iter().chain(bad_5).collect();
    bad.sort_unstable();
    bad.dedup();

    let ok = bad.is_empty();
    let summary = if ok {
        format!(
            "Full check passed: {} survived both patterns",
            usbooty_core::device::format_size(size)
        )
    } else {
        format!("Full check FAILED: {} bad sector(s) found", bad.len())
    };
    Ok(CheckReport {
        ok,
        bad_offsets: bad,
        effective_capacity: None,
        summary,
    })
}

/// Write `pattern` across the whole device, then read it back. Any sector
/// (4 KiB aligned) that does not match is added to the returned list.
fn scan_pattern(
    dev: &mut std::fs::File,
    size: u64,
    pattern: u8,
    phase: &str,
    abort: &AtomicBool,
) -> Result<Vec<u64>> {
    emit::phase(phase);
    let buf = vec![pattern; FULL_BUF];
    let mut done = 0u64;
    let mut last = Instant::now();
    dev.seek(SeekFrom::Start(0)).context("seeking the device")?;
    while done < size {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let chunk = ((size - done) as usize).min(buf.len());
        dev.write_all(&buf[..chunk])
            .context("writing the test pattern")?;
        done += chunk as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress(phase, done, size);
            last = Instant::now();
        }
    }
    dev.flush().context("flushing the test pattern")?;
    nix::unistd::fsync(&*dev).context("syncing the test pattern to the device")?;
    // Invalidate the page cache so the read-back hits the media, not the
    // pages the write pass just dirtied.
    blockdev::flush_page_cache(dev)?;

    // Read-back pass: hashes are pointless here (we only care *where* it
    // went wrong); compare the buffer byte-for-byte and record mismatches.
    let mut read_buf = vec![0u8; FULL_BUF];
    let mut bad = Vec::new();
    done = 0;
    dev.seek(SeekFrom::Start(0)).context("seeking the device")?;
    while done < size {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let chunk = ((size - done) as usize).min(read_buf.len());
        dev.read_exact(&mut read_buf[..chunk])
            .context("reading the device back")?;
        diff_sectors(&read_buf[..chunk], &buf[..chunk], done, &mut bad);
        done += chunk as u64;
        if last.elapsed() >= REPORT_EVERY {
            emit::progress(phase, done, size);
            last = Instant::now();
        }
    }
    Ok(bad)
}

/// Compare a read-back chunk against the expected pattern buffer block by
/// block, appending the device offset of every mismatching 4 KiB sector to
/// `bad`. `base` is the device offset the chunk starts at. The trailing block
/// may be shorter than `BLOCK`; `sector.len()` keeps the pattern slice in step.
fn diff_sectors(read: &[u8], pattern: &[u8], base: u64, bad: &mut Vec<u64>) {
    debug_assert_eq!(read.len(), pattern.len());
    // Fast path: the common case is a clean chunk, so skip the per-sector walk.
    if read == pattern {
        return;
    }
    for (i, sector) in read.chunks(BLOCK).enumerate() {
        let off = i * BLOCK;
        if sector != &pattern[off..off + sector.len()] {
            bad.push(base + off as u64);
        }
    }
}

/// Return `n` evenly-spaced 4 KiB-aligned offsets in `[0, size - BLOCK]`.
fn sample_offsets(size: u64, n: u64) -> Vec<u64> {
    let last = size.saturating_sub(BLOCK as u64);
    if last == 0 || n == 0 {
        return vec![0];
    }
    let n = n.min(last / BLOCK as u64 + 1);
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        // Distribute evenly across the device, rounding each to a 4 KiB boundary.
        let off = (i * last) / (n - 1).max(1);
        out.push(off / BLOCK as u64 * BLOCK as u64);
    }
    out
}

/// Fill `buf` with a 4 KiB fingerprint deterministically derived from
/// `(seed, offset)`. The XorShift64* output gives uniform-looking bytes
/// without pulling in an RNG crate.
fn fill_fingerprint(buf: &mut [u8], seed: u64, offset: u64) {
    let mut state = seed ^ offset.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // Header carries the parameters so the read-back can detect "wrong but
    // structured" content (e.g. a stale write from an earlier run).
    buf[..8].copy_from_slice(&seed.to_le_bytes());
    buf[8..16].copy_from_slice(&offset.to_le_bytes());
    for chunk in buf[16..].chunks_mut(8) {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let v = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let bytes = v.to_le_bytes();
        for (b, src) in chunk.iter_mut().zip(bytes.iter()) {
            *b = *src;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_offsets_returns_zero_for_tiny_device() {
        // Smaller than one BLOCK: only valid offset is 0.
        assert_eq!(sample_offsets(0, 32), vec![0]);
        assert_eq!(sample_offsets(BLOCK as u64 - 1, 32), vec![0]);
    }

    #[test]
    fn sample_offsets_spans_the_device_block_aligned() {
        let size = 16 * 1024 * 1024;
        let offs = sample_offsets(size, 8);
        // At least the first offset is 0 and every offset is BLOCK-aligned
        // and within bounds.
        assert_eq!(offs.first().copied(), Some(0));
        for &o in &offs {
            assert!(o + BLOCK as u64 <= size, "offset {o} past device end");
            assert_eq!(o % BLOCK as u64, 0, "offset {o} not 4 KiB aligned");
        }
    }

    #[test]
    fn fill_fingerprint_is_deterministic() {
        let mut a = [0u8; 4096];
        let mut b = [0u8; 4096];
        fill_fingerprint(&mut a, 0xdead, 0xbeef);
        fill_fingerprint(&mut b, 0xdead, 0xbeef);
        assert_eq!(a, b);
    }

    #[test]
    fn fill_fingerprint_header_encodes_seed_and_offset() {
        let mut buf = [0u8; 4096];
        fill_fingerprint(&mut buf, 0x1122_3344_5566_7788, 0x99AA_BBCC_DDEE_FF00);
        assert_eq!(
            u64::from_le_bytes(buf[..8].try_into().unwrap()),
            0x1122_3344_5566_7788
        );
        assert_eq!(
            u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            0x99AA_BBCC_DDEE_FF00
        );
    }

    #[test]
    fn diff_sectors_reports_each_corrupt_block_at_its_own_offset() {
        // Three blocks of pattern; corrupt the *middle* one. A naive
        // implementation that always compared against the first block would
        // either miss this or flag the wrong offset.
        let pattern = vec![0xAAu8; 3 * BLOCK];
        let mut read = pattern.clone();
        read[BLOCK + 17] = 0x55; // single bad byte inside block 1
        let mut bad = Vec::new();
        diff_sectors(&read, &pattern, 1_000_000, &mut bad);
        assert_eq!(bad, vec![1_000_000 + BLOCK as u64]);
    }

    #[test]
    fn diff_sectors_clean_chunk_reports_nothing() {
        let pattern = vec![0x5Au8; 2 * BLOCK];
        let read = pattern.clone();
        let mut bad = Vec::new();
        diff_sectors(&read, &pattern, 0, &mut bad);
        assert!(bad.is_empty());
    }

    #[test]
    fn diff_sectors_handles_short_final_block() {
        // Last block is a partial sector; a corrupt byte in it must still be
        // reported at the right offset without panicking on the slice.
        let pattern = vec![0xFFu8; BLOCK + 512];
        let mut read = pattern.clone();
        read[BLOCK + 100] = 0x00;
        let mut bad = Vec::new();
        diff_sectors(&read, &pattern, 0, &mut bad);
        assert_eq!(bad, vec![BLOCK as u64]);
    }

    #[test]
    fn fill_fingerprint_changes_on_different_offset() {
        let mut a = [0u8; 4096];
        let mut b = [0u8; 4096];
        fill_fingerprint(&mut a, 0xdead, 0);
        fill_fingerprint(&mut b, 0xdead, BLOCK as u64);
        assert_ne!(a, b, "different offset should produce different bytes");
    }
}
