//! Apply a Windows image out of an `install.wim` / `install.esd` onto a
//! formatted NTFS partition via `wimlib-imagex apply`.
//!
//! This is the payload step of the Windows To Go flow: unlike the installer
//! path (which copies `install.wim` verbatim or splits it), WTG *extracts* one
//! chosen edition into a live, bootable Windows directory tree on the NTFS
//! partition. `--rpfix` rewrites absolute-path reparse points (junctions) so
//! they resolve against the new volume root rather than the build machine's.

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::{emit, fsutil};

/// How often to poll `abort` while wimlib-imagex runs.
const POLL_EVERY: Duration = Duration::from_millis(250);

/// The phase name surfaced to the GUI while the image extracts.
const PHASE: &str = "Applying Windows image";

/// Apply image `index` from `wim` (a `install.wim`/`install.esd`) into the
/// directory `dest` (a mounted, NTFS-formatted partition). Long-running and
/// cancellable: a 5 GiB edition takes many minutes, so the apply runs as a
/// child process polled against `abort`. Progress percentages parsed from
/// wimlib's output are forwarded to the GUI.
pub fn apply_image(wim: &Path, index: u32, dest: &Path, abort: &AtomicBool) -> Result<()> {
    if !fsutil::wimlib_available() {
        bail!(
            "wimlib-imagex is required for Windows To Go; install the \
             `wimlib` / `wimtools` package and try again"
        );
    }

    emit::phase(PHASE);
    emit::log(format!(
        "Applying image #{index} from {} to {}",
        wim.display(),
        dest.display()
    ));

    let mut child = Command::new("wimlib-imagex")
        .arg("apply")
        .arg(wim)
        .arg(index.to_string())
        .arg(dest)
        // Fix up absolute-target junctions/symlinks to the new volume root.
        .arg("--rpfix")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning wimlib-imagex")?;

    // wimlib writes progress to stdout as carriage-return-updated lines like
    // "Extracting file data: 1234 MiB of 5678 MiB (21%) done". Read it on a
    // side thread and forward the percentage; the main thread owns abort/wait.
    let progress_thread = child.stdout.take().map(|stdout| {
        thread::spawn(move || forward_progress(stdout))
    });

    let status = loop {
        if abort.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(t) = progress_thread {
                let _ = t.join();
            }
            bail!("aborted by user");
        }
        match child.try_wait().context("waiting for wimlib-imagex")? {
            Some(status) => break status,
            None => thread::sleep(POLL_EVERY),
        }
    };

    if let Some(t) = progress_thread {
        let _ = t.join();
    }

    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }
        bail!("wimlib-imagex apply failed: {}", stderr.trim());
    }
    emit::log("Windows image applied to the NTFS partition");
    Ok(())
}

/// Read wimlib's progress stream and forward progress as a
/// [`ProgressMsg`](usbooty_core::ProgressMsg). Segments are delimited by either
/// `\r` (in-place progress updates) or `\n`.
///
/// wimlib prints lines like `"Extracting file data: 1234 MiB of 5678 MiB (21%)
/// done"`. We prefer the real `done / total` *byte* figures so the GUI can show
/// an accurate speed and ETA, falling back to the bare percentage only when no
/// byte pair is present.
fn forward_progress<R: Read>(mut stream: R) {
    let mut buf = [0u8; 4096];
    let mut seg = String::new();
    let mut last_done = u64::MAX;
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                if let Some((done, total)) = parse_amounts(&seg) {
                    if done != last_done {
                        last_done = done;
                        emit::progress(PHASE, done, total);
                    }
                } else if let Some(pct) = parse_percent(&seg)
                    && pct != last_done
                {
                    last_done = pct;
                    emit::progress(PHASE, pct, 100);
                }
                seg.clear();
            } else {
                seg.push(b as char);
            }
        }
    }
}

/// Parse the `"<n> <unit> of <n> <unit>"` byte pair out of a wimlib progress
/// segment, returning `(done, total)` in bytes. Honors `KiB`/`MiB`/`GiB` (and a
/// bare `B`/`bytes`). Returns `None` if the segment has no such pair.
fn parse_amounts(seg: &str) -> Option<(u64, u64)> {
    let of = seg.find(" of ")?;
    let done = parse_amount(&seg[..of])?;
    let total = parse_amount(&seg[of + 4..])?;
    Some((done, total))
}

/// Parse the last `"<number> <unit>"` amount in `s` into bytes. The number is
/// taken as the digits immediately preceding the first recognized unit word.
fn parse_amount(s: &str) -> Option<u64> {
    const UNITS: [(&str, u64); 5] = [
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
        ("bytes", 1),
        ("B", 1),
    ];
    let (unit_pos, mult) = UNITS
        .iter()
        .filter_map(|&(name, mult)| s.find(name).map(|pos| (pos, mult)))
        .min_by_key(|&(pos, _)| pos)?;
    let digits: String = s[..unit_pos]
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<u64>().ok().map(|n| n * mult)
}

/// Extract the integer percentage from a segment like `"... (21%) done"`.
fn parse_percent(seg: &str) -> Option<u64> {
    let pct_pos = seg.find('%')?;
    let digits: String = seg[..pct_pos]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok().filter(|&p| p <= 100)
}

#[cfg(test)]
mod tests {
    use super::{parse_amounts, parse_percent};

    #[test]
    fn parses_wimlib_percent_lines() {
        assert_eq!(
            parse_percent("Extracting file data: 1234 MiB of 5678 MiB (21%) done"),
            Some(21)
        );
        assert_eq!(parse_percent("100% complete"), Some(100));
        assert_eq!(parse_percent("no percentage here"), None);
        assert_eq!(parse_percent("(150%) bogus"), None);
    }

    #[test]
    fn parses_wimlib_byte_amounts() {
        assert_eq!(
            parse_amounts("Extracting file data: 1234 MiB of 5678 MiB (21%) done"),
            Some((1234 << 20, 5678 << 20))
        );
        assert_eq!(
            parse_amounts("Extracting file data: 2 GiB of 16 GiB (12%) done"),
            Some((2 << 30, 16 << 30))
        );
        assert_eq!(
            parse_amounts("Extracting file data: 512 KiB of 1024 KiB (50%) done"),
            Some((512 << 10, 1024 << 10))
        );
        // No "of" pair -> percentage fallback path is used instead.
        assert_eq!(parse_amounts("100% complete"), None);
        assert_eq!(parse_amounts("Creating directories"), None);
    }
}
