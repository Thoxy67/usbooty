//! Pure, QObject-free helpers shared by the `AppController` method modules.

use usbooty_core::DeviceInfo;

/// `Some(trimmed)` when `s` has any non-whitespace content; `None` otherwise.
///
/// The unattend generator treats `Some("")` the same as `None`, but using
/// `None` keeps the resulting JSON tidy and round-trips cleanly.
pub(crate) fn trimmed_opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// `Some(s)` when `s` is non-empty *without* trimming; passwords keep their
/// leading and trailing whitespace because Windows compares them exactly.
pub(crate) fn non_empty_opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Convert a `file://` URL string (what QML's FileDialog and drag-drop hand
/// us) into a local filesystem path, percent-decoding any escaped bytes
/// (`%23` for `#`, `%25` for `%`, ...). A plain path passes through
/// unchanged: `%` sequences are only decoded after a `file://` prefix was
/// actually present, so a local file literally named `100%23.iso` survives.
pub(crate) fn local_path_from_url(raw: &str) -> String {
    match raw.strip_prefix("file://") {
        Some(stripped) => percent_decode(stripped),
        None => raw.to_string(),
    }
}

/// Decode `%XX` percent-escapes; malformed sequences pass through verbatim.
fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        (b as char).to_digit(16).map(|d| d as u8)
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Worker for [`AppController::request_inspect`]: shell out to lsblk,
/// udevadm and smartctl, collate the output. Pure function (only depends
/// on the device path) so it can run on a worker thread without touching
/// any QObject state.
pub(crate) fn collect_inspect_text(path: &str) -> String {
    // lsblk: passing both `-O` (all columns) and `--output` blanks the
    // output on most versions, so pick a useful column set explicitly.
    // Dropping `-d` keeps the disk + its partitions, which is exactly
    // what the user wants to see before erasing.
    let lsblk = std::process::Command::new("lsblk")
        .args([
            "-p",
            "--output",
            "NAME,SIZE,TYPE,FSTYPE,LABEL,UUID,PARTLABEL,MOUNTPOINTS,MODEL,VENDOR,TRAN,REV,ROTA,RM,RO,HOTPLUG",
        ])
        .arg(path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|e| format!("(lsblk failed: {e})"));
    let udev_raw = std::process::Command::new("udevadm")
        .args(["info", "--query=property", "--name"])
        .arg(path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|e| format!("(udevadm failed: {e})"));
    let udev = clean_udev(&udev_raw);
    // smartctl: info + overall health + attribute table is the
    // "is this drive ok?" subset. Self-test and error logs would bloat
    // the panel for no everyday benefit. Exit code is non-zero whenever
    // SMART is unsupported or permission is denied (both common), so
    // the panel inspects stderr to give a useful message either way.
    let smart = match std::process::Command::new("smartctl")
        .args(["-i", "-H", "-A"])
        .arg(path)
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = stdout.trim().to_string();
            if combined.contains("Permission denied") || stderr.contains("Permission denied") {
                "(smartctl needs root for raw device access. Either run\n \
                   sudo chmod u+s $(which smartctl)\n \
                 once to setuid the binary, or launch usbooty with sudo.)"
                    .to_string()
            } else if combined.is_empty() {
                let tail = stderr.trim();
                if tail.is_empty() {
                    "(smartctl returned no output; device may not expose SMART)".to_string()
                } else {
                    format!("(smartctl: {tail})")
                }
            } else {
                combined
            }
        }
        Err(_) => "(smartctl not installed; install the `smartmontools` package \
                   to see SMART health here)"
            .to_string(),
    };
    format!(
        "── lsblk ───────────────────────────────────────────\n{lsblk}\n\
         ── udevadm ─────────────────────────────────────────\n{udev}\n\
         ── smartctl ────────────────────────────────────────\n{smart}"
    )
}

/// Strip noisy duplicates from a `udevadm info --query=property` dump:
/// every `ID_FOO_ENC=…` is the same value as `ID_FOO=…` with spaces and
/// other bytes hex-encoded, so removing them roughly halves the output
/// without losing any information.
fn clean_udev(raw: &str) -> String {
    raw.lines()
        .filter(|line| {
            let key = line.split('=').next().unwrap_or("");
            !key.ends_with("_ENC")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collect every mounted source under `device_path` from `/proc/mounts`:
/// the whole-disk node itself and any `sdc1`, `sdc12`, `nvme0n1p1`, …
/// partition. Same prefix-then-digit / `p`-then-digit match the previous
/// `is_device_mounted` used, just returning every hit instead of the first.
fn mounted_sources(device_path: &str) -> Vec<String> {
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in mounts.lines() {
        let Some(source) = line.split_whitespace().next() else {
            continue;
        };
        if source == device_path {
            out.push(source.to_string());
            continue;
        }
        if let Some(tail) = source.strip_prefix(device_path) {
            let first = tail.chars().next();
            if first.is_some_and(|c| c.is_ascii_digit())
                || tail
                    .strip_prefix('p')
                    .is_some_and(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))
            {
                out.push(source.to_string());
            }
        }
    }
    out
}

/// Pure version of [`AppController::max_persistence_mib`], used by both the
/// property refresher and the legacy invokable so the two can never drift
/// apart. Returns the largest persistence partition size that still leaves
/// room for `iso` plus a 64 MiB filesystem / partition-table margin on
/// `device`. `0` when there is no device, or no headroom (the slider
/// should stay hidden in that case).
pub(crate) fn compute_max_persistence_mib(
    device: Option<&DeviceInfo>,
    iso: Option<&usbooty_core::IsoReport>,
) -> i32 {
    let Some(device) = device else {
        return 0;
    };
    let iso_size = iso.map_or(0u64, |r| r.total_size);
    const MARGIN: u64 = 64 * 1024 * 1024;
    let usable = device.size.saturating_sub(iso_size).saturating_sub(MARGIN);
    let mib = usable / (1024 * 1024);
    i32::try_from(mib).unwrap_or(i32::MAX)
}

/// Unmount every mounted partition of `device_path` via `udisksctl unmount`,
/// which runs as the user and tells the desktop session to release the mount
/// cleanly. Returns `Ok(_)` when no partitions remain mounted afterwards
/// (whether we had to unmount any or not); returns `Err(_)` with the
/// `udisksctl` stderr from the first failing partition otherwise.
pub(crate) fn unmount_device_partitions(device_path: &str) -> Result<usize, String> {
    let sources = mounted_sources(device_path);
    let mut unmounted = 0_usize;
    for source in sources {
        let out = std::process::Command::new("udisksctl")
            .args(["unmount", "-b", &source])
            .output()
            .map_err(|e| format!("running udisksctl: {e}."))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("{source}: {stderr}"));
        }
        unmounted += 1;
    }
    Ok(unmounted)
}

#[cfg(test)]
mod tests {
    use super::local_path_from_url;

    #[test]
    fn file_urls_are_stripped_and_percent_decoded() {
        assert_eq!(
            local_path_from_url("file:///home/u/My%20ISOs/x%2364.iso"),
            "/home/u/My ISOs/x#64.iso"
        );
        assert_eq!(
            local_path_from_url("file:///plain/path.iso"),
            "/plain/path.iso"
        );
        // Plain paths pass through untouched, escapes included.
        assert_eq!(
            local_path_from_url("/literal/100%23.iso"),
            "/literal/100%23.iso"
        );
        // Malformed escapes survive verbatim.
        assert_eq!(local_path_from_url("file:///a%2.iso"), "/a%2.iso");
    }
}
