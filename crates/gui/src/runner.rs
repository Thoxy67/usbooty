//! Spawns the privileged helper via `pkexec` and pumps its progress stream
//! back onto the Qt thread.
//!
//! This runs entirely on a worker thread. It never touches QObject state
//! directly; every UI mutation is marshalled through [`CxxQtThread::queue`],
//! which runs the closure on the Qt main thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use core::pin::Pin;
use cxx_qt::{CxxQtThread, CxxQtType};
use cxx_qt_lib::QString;
use usbooty_core::{Job, LogLevel, ProgressMsg, WimStrategy};

use crate::bridge::qobject::AppController;
use crate::resources::{self, Resource};

/// Installed location of the privileged helper.
const INSTALLED_HELPER: &str = "/usr/libexec/usbooty/usbooty-helper";

/// Locate `usbooty-helper`: next to this executable for a dev build, otherwise
/// the installed path.
fn helper_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(local) = exe.parent().map(|d| d.join("usbooty-helper"))
        && local.is_file()
    {
        return local;
    }
    PathBuf::from(INSTALLED_HELPER)
}

/// A smoothed transfer-rate estimator shared by the writer and the downloader.
struct RateMeter {
    /// When the transfer began, the basis for elapsed time and average rate.
    start: Instant,
    /// The most recent `(timestamp, bytes)` anchor for the windowed rate.
    anchor: (Instant, u64),
    /// Exponentially-smoothed instantaneous rate, in bytes per second.
    rate: f64,
}

impl RateMeter {
    fn new() -> Self {
        let now = Instant::now();
        RateMeter {
            start: now,
            anchor: (now, 0),
            rate: 0.0,
        }
    }

    /// Forget the windowed estimate for a new phase, keeping `start` so the
    /// job-wide elapsed time in the final summary stays intact. Called on every
    /// phase boundary so a previous phase's speed/ETA never lingers on screen.
    fn reset(&mut self) {
        let now = Instant::now();
        self.anchor = (now, 0);
        self.rate = 0.0;
    }

    /// Feed the latest cumulative byte count and return the smoothed rate.
    /// A drop in `done`, a new phase that restarts at zero, resets the
    /// estimate so one phase's speed never leaks into the next.
    fn sample(&mut self, done: u64) -> f64 {
        let now = Instant::now();
        if done < self.anchor.1 {
            self.anchor = (now, done);
            self.rate = 0.0;
            return 0.0;
        }
        let dt = now.duration_since(self.anchor.0).as_secs_f64();
        if dt >= 0.5 {
            let instant = (done - self.anchor.1) as f64 / dt;
            self.rate = if self.rate == 0.0 {
                instant
            } else {
                0.65 * self.rate + 0.35 * instant
            };
            self.anchor = (now, done);
        }
        self.rate
    }
}

/// Format a bytes-per-second rate like `48.2 MB/s` (empty when not moving).
fn format_rate(bps: f64) -> String {
    if bps < 1.0 {
        return String::new();
    }
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let (mut value, mut unit) = (bps, 0);
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Format a duration in seconds as `1h 04m`, `2m 12s`, or `38s`.
fn format_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Compute the ETA string for the bytes still to transfer at `rate` B/s.
fn eta_string(rate: f64, done: u64, total: u64) -> String {
    if rate < 1.0 || total <= done {
        return String::new();
    }
    format_duration(((total - done) as f64 / rate) as u64)
}

/// Push the live `speed` / `eta` properties onto the Qt thread.
fn push_stats(qt: &CxxQtThread<AppController>, rate: f64, done: u64, total: u64) {
    let speed = format_rate(rate);
    let eta = eta_string(rate, done, total);
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_speed(QString::from(&speed));
        ctrl.as_mut().set_eta(QString::from(&eta));
    });
}

/// Run `job` to completion, forwarding progress to the `AppController`.
///
/// `cancel` is the job handle's flag, set by `AppController::cancel`. It is
/// checked before the resource downloads and around the helper spawn, the
/// stretches during which the helper's stdin (the normal cancel channel)
/// does not exist yet, so a cancel clicked there is not silently dropped.
pub fn run_job(
    mut job: Job,
    qt: CxxQtThread<AppController>,
    stdin_slot: Arc<Mutex<Option<ChildStdin>>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;

    let cancelled = || cancel.load(Ordering::SeqCst);
    if cancelled() {
        finish(&qt, false, "Cancelled.".into());
        return;
    }

    // The UEFI:NTFS layout needs the bootloader image; download it (off the
    // Qt thread) and hand the helper a local path, so the root helper itself
    // never needs network access.
    if let Job::Partitioned {
        wim: WimStrategy::UefiNtfs,
        uefi_ntfs_img: img @ None,
        ..
    } = &mut job
    {
        apply(
            &qt,
            ProgressMsg::info("Fetching the UEFI:NTFS bootloader image…"),
        );
        match resources::ensure(Resource::UefiNtfsImg) {
            Ok(path) => *img = Some(path),
            Err(e) => {
                finish(&qt, false, format!("{e:#}"));
                return;
            }
        }
    }

    // FreeDOS: download the kernel + command-shell zips (latest GitHub
    // release per repo), extract the three files we need, and hand the
    // helper the cached paths. Same justification as UEFI:NTFS: the
    // helper has no network access.
    if let Job::Freedos {
        filesystem,
        kernel_sys,
        command_com,
        boot_bin,
        ..
    } = &mut job
        && kernel_sys.as_os_str().is_empty()
    {
        apply(
            &qt,
            ProgressMsg::info("Fetching the latest FreeDOS kernel + shell…"),
        );
        let fat32 = matches!(*filesystem, usbooty_core::FileSystem::Fat32);
        match resources::ensure_freedos(fat32) {
            Ok(files) => {
                *kernel_sys = files.kernel_sys;
                *command_com = files.command_com;
                *boot_bin = files.boot_bin;
            }
            Err(e) => {
                finish(&qt, false, format!("FreeDOS fetch failed: {e:#}"));
                return;
            }
        }
    }

    // The resource downloads above can take a while; honour a cancel that
    // arrived during them instead of going on to spawn the helper.
    if cancelled() {
        finish(&qt, false, "Cancelled.".into());
        return;
    }

    let helper = helper_path();
    let mut child = match Command::new("pkexec")
        .arg(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            finish(&qt, false, format!("Could not launch pkexec: {e}"));
            return;
        }
    };

    // Drain stderr continuously on its own thread. Only reading it after the
    // stdout loop ends would deadlock the job if the helper (or a tool whose
    // stderr it inherits: mkfs, ventoy, pkexec itself) fills the ~64 KiB pipe
    // buffer.
    let stderr_drain = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });

    // Send the job as one JSON line, then keep stdin open so `cancel` can be
    // written to it later by AppController::cancel. Keeping it open is part
    // of the helper's protocol, not just a convenience: the helper treats
    // EOF on its stdin as "the GUI died, abort", so the ChildStdin parked in
    // the job handle must stay alive until the job finishes (finish() clears
    // the handle, which is what finally closes the pipe).
    let Some(mut stdin) = child.stdin.take() else {
        reap(&mut child);
        finish(&qt, false, "Could not open the helper's stdin pipe".into());
        return;
    };
    let job_line = serde_json::to_string(&job).unwrap_or_default();
    if writeln!(stdin, "{job_line}")
        .and_then(|_| stdin.flush())
        .is_err()
    {
        reap(&mut child);
        finish(&qt, false, "Could not send the job to the helper".into());
        return;
    }
    *stdin_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(stdin);

    // Close the race with a cancel that fired after `cancel()` last looked at
    // the (then-empty) stdin slot but before the helper existed: now that the
    // slot is populated, forward the flag as the helper's `cancel` line.
    if cancelled()
        && let Ok(mut guard) = stdin_slot.lock()
        && let Some(stdin) = guard.as_mut()
    {
        let _ = writeln!(stdin, "cancel");
        let _ = stdin.flush();
    }

    // Stream stdout, forwarding each progress message to the UI.
    let Some(stdout) = child.stdout.take() else {
        reap(&mut child);
        finish(&qt, false, "Could not open the helper's stdout pipe".into());
        return;
    };
    let mut last_error: Option<String> = None;
    let mut saw_done = false;
    let mut meter = RateMeter::new();
    // Bytes moved in the current phase, and the sum of completed phases' peaks,
    // so the final-summary average reflects the whole job rather than a single
    // phase's count divided by the whole-job elapsed time.
    let mut phase_peak = 0u64;
    let mut job_total = 0u64;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<ProgressMsg>(&line) else {
            continue; // ignore non-protocol noise
        };
        match &msg {
            ProgressMsg::Done => saw_done = true,
            ProgressMsg::Error { text } => last_error = Some(text.clone()),
            ProgressMsg::Phase { .. } => {
                // A new phase: bank this phase's bytes, reset the rate estimate
                // and clear the stale speed/ETA so they don't freeze on screen
                // through phases that report no byte progress (Formatting,
                // Flushing, boot-file copy, …).
                job_total += phase_peak;
                phase_peak = 0;
                meter.reset();
                push_stats(&qt, 0.0, 0, 0);
            }
            ProgressMsg::Progress { done, total, .. } => {
                phase_peak = phase_peak.max(*done);
                let rate = meter.sample(*done);
                push_stats(&qt, rate, *done, *total);
            }
            _ => {}
        }
        apply(&qt, msg);
    }

    let stderr = stderr_drain
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let (success, message) = outcome(&mut child, saw_done, last_error, &stderr);
    let message = finish_summary(success, message, &meter, job_total + phase_peak);

    // Post-job convenience: a freshly-prepared Ventoy stick with no source
    // ISO is most useful with the data partition open, so the user can drop
    // their ISOs straight onto it. Mount it (via the user's normal
    // udisksctl plumbing) and pop it open in their file manager.
    if success
        && let Job::Ventoy {
            iso_path: None,
            device_path,
            ..
        } = &job
    {
        apply(
            &qt,
            ProgressMsg::info("Opening the Ventoy data partition in your file manager…"),
        );
        open_ventoy_data_partition(device_path);
    }

    finish(&qt, success, message);
}

/// Mount the first (data) partition of a freshly-prepared Ventoy device and
/// open it in the user's file manager. All steps are best-effort and
/// silent on failure; if `udisksctl` or `xdg-open` are missing, or the
/// kernel hasn't re-read the partition table yet, the user just doesn't
/// get the auto-open and can mount manually.
fn open_ventoy_data_partition(device_path: &std::path::Path) {
    // Give the kernel a moment to surface the new partitions after Ventoy
    // rewrites the table; without `udevadm settle` `udisksctl` often races
    // and returns "No such interface".
    let _ = Command::new("udevadm")
        .args(["settle", "--timeout=5"])
        .status();

    let part = first_partition_path(device_path);

    // Try to mount via udisksctl (the freedesktop way). When the partition
    // is already mounted (Ventoy's own update path often auto-mounts), the
    // call fails; fall back to findmnt to discover where it ended up.
    let mount_output = Command::new("udisksctl")
        .args(["mount", "-b"])
        .arg(&part)
        .output();
    let mut mount_point: Option<String> = mount_output.ok().and_then(|out| {
        // udisksctl prints: "Mounted /dev/sdb1 at /run/media/user/Ventoy."
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout
            .lines()
            .find_map(|line| line.split(" at ").nth(1))
            .map(|s| s.trim_end_matches('.').trim().to_string())
    });
    if mount_point.is_none() {
        mount_point = Command::new("findmnt")
            .args(["-n", "-o", "TARGET"])
            .arg(&part)
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                (!s.is_empty()).then_some(s)
            });
    }

    if let Some(mp) = mount_point
        && let Err(e) = Command::new("xdg-open").arg(&mp).spawn()
    {
        eprintln!("usbooty: could not open {mp} in a file manager: {e}");
    }
}

/// Map a whole-disk device node to the path of its first partition.
/// `nvme0n1` / `mmcblk0` get a `p` suffix; everything else (sd*, vd*, …)
/// just appends the digit.
fn first_partition_path(device: &std::path::Path) -> std::path::PathBuf {
    let s = device.to_string_lossy();
    let needs_p = s
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("nvme") || name.starts_with("mmcblk"));
    if needs_p {
        std::path::PathBuf::from(format!("{s}p1"))
    } else {
        std::path::PathBuf::from(format!("{s}1"))
    }
}

/// Append elapsed time and average rate to a successful job's message.
fn finish_summary(success: bool, message: String, meter: &RateMeter, moved: u64) -> String {
    let elapsed = meter.start.elapsed().as_secs();
    if success && moved > 0 && elapsed > 0 {
        format!(
            "{message} ({} elapsed, {} average)",
            format_duration(elapsed),
            format_rate(moved as f64 / elapsed as f64),
        )
    } else {
        message
    }
}

/// Kill and wait a child whose setup failed. Dropping a `Child` without
/// waiting leaves a zombie process around for the rest of the GUI session.
fn reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Wait for the helper and decide the final success flag and message.
/// `stderr` is the output collected by the drain thread.
fn outcome(
    child: &mut Child,
    saw_done: bool,
    last_error: Option<String>,
    stderr: &str,
) -> (bool, String) {
    let status = child.wait();
    let success = saw_done && matches!(&status, Ok(s) if s.success());

    if success {
        return (true, "Done. The drive is ready".into());
    }
    if let Some(err) = last_error {
        return (false, err);
    }

    let message = match status {
        Ok(s) if !stderr.trim().is_empty() => format!("Helper failed ({s}): {}", stderr.trim()),
        Ok(s) => format!("Helper exited unexpectedly ({s})"),
        Err(e) => format!("Helper could not be waited on: {e}"),
    };
    (false, message)
}

/// Marshal a single progress message onto the Qt thread.
fn apply(qt: &CxxQtThread<AppController>, msg: ProgressMsg) {
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| match msg {
        ProgressMsg::Log { level, text } => append_log(ctrl, level, &text),
        ProgressMsg::Phase { name } => {
            ctrl.as_mut().set_phase(QString::from(&name));
            append_phase_header(ctrl, &name);
        }
        ProgressMsg::Progress { phase, done, total } => {
            let fraction = if total > 0 {
                done as f64 / total as f64
            } else {
                0.0
            };
            ctrl.as_mut().set_phase(QString::from(&phase));
            ctrl.as_mut().set_progress(fraction);
        }
        ProgressMsg::Done => {}
        ProgressMsg::Error { text } => append_log(ctrl, LogLevel::Error, &text),
    });
}

/// Report the terminal job state on the Qt thread.
fn finish(qt: &CxxQtThread<AppController>, success: bool, message: String) {
    // Fire-and-forget desktop notification: lets the user switch windows
    // during long jobs and still hear about completion. notify-send ships
    // with every freedesktop notifier (gnome-shell, KDE, dunst, mako, …);
    // missing = silently skipped.
    notify(success, &message);
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        // Record the outcome in the log itself (bold green / red), so a saved
        // log ends with a clear verdict instead of trailing off mid-stream.
        ctrl.as_mut()
            .push_log_line(&message, &outcome_html(success, &message));
        ctrl.as_mut().set_busy(false);
        ctrl.as_mut()
            .set_phase(QString::from(if success { "Finished" } else { "Failed" }));
        // Pin the bar to either end so a half-full failure bar doesn't
        // linger after the job has clearly stopped.
        ctrl.as_mut().set_progress(if success { 1.0 } else { 0.0 });
        ctrl.as_mut().set_speed(QString::default());
        ctrl.as_mut().set_eta(QString::default());
        ctrl.as_mut().set_status(QString::from(&message));
        ctrl.as_mut().rust_mut().job = None;
        ctrl.as_mut().job_finished(success, QString::from(&message));
    });
}

/// Spawn `notify-send` with a usbooty-themed title + body so the result is
/// visible even when the GUI is minimized or behind another window. Silent
/// failure if notify-send is unavailable; the in-app dialog is the
/// authoritative report.
fn notify(success: bool, message: &str) {
    let urgency = if success { "normal" } else { "critical" };
    let title = if success {
        "usbooty: Finished"
    } else {
        "usbooty: Failed"
    };
    let icon = if success { "dialog-ok" } else { "dialog-error" };
    let _ = std::process::Command::new("notify-send")
        .args([
            "--app-name=usbooty",
            "--urgency",
            urgency,
            "--icon",
            icon,
            title,
            message,
        ])
        .spawn();
}

/// Fetch the language list for a Windows release and publish it to the UI.
pub fn win_fetch_languages(qt: CxxQtThread<AppController>, edition_id: u32) {
    match crate::windisco::fetch_languages(edition_id) {
        Ok(catalog) => {
            let names = catalog.language_names().join("\n");
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_status(QString::from("Select a language"));
                ctrl.as_mut().set_win_languages(QString::from(&names));
                ctrl.as_mut().set_win_options(QString::default());
                ctrl.as_mut().rust_mut().win_catalog = Some(catalog);
                ctrl.as_mut().rust_mut().win_option_list.clear();
            });
        }
        Err(e) => finish(&qt, false, format!("{e:#}")),
    }
}

/// Fetch the download options for one language and publish them to the UI.
pub fn win_fetch_options(
    qt: CxxQtThread<AppController>,
    catalog: crate::windisco::Catalog,
    language_index: usize,
) {
    match catalog.fetch_options(language_index) {
        Ok(options) => {
            let labels = options
                .iter()
                .map(|o| o.label.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_status(QString::from("Select a download"));
                ctrl.as_mut().set_win_options(QString::from(&labels));
                ctrl.as_mut().rust_mut().win_option_list = options;
            });
        }
        Err(e) => finish(&qt, false, format!("{e:#}")),
    }
}

/// Download a Windows ISO from `url` and select it as the source. `abort` is
/// flipped by [`crate::bridge::AppController::cancel`] when the user clicks
/// Cancel; `windisco::download` polls it between chunks.
pub fn download_windows_url(
    qt: CxxQtThread<AppController>,
    url: String,
    abort: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    set_phase(&qt, "Downloading Windows ISO");
    apply(
        &qt,
        ProgressMsg::info("Downloading the Windows ISO from Microsoft…"),
    );

    let dest_dir = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(std::path::Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir);

    let qt_progress = qt.clone();
    let mut meter = RateMeter::new();
    let result = crate::windisco::download(&url, &dest_dir, &abort, |done, total| {
        let rate = meter.sample(done);
        let fraction = if total > 0 {
            done as f64 / total as f64
        } else {
            0.0
        };
        let speed = format_rate(rate);
        let eta = eta_string(rate, done, total);
        let _ = qt_progress.queue(move |mut ctrl: Pin<&mut AppController>| {
            ctrl.as_mut().set_progress(fraction);
            ctrl.as_mut().set_speed(QString::from(&speed));
            ctrl.as_mut().set_eta(QString::from(&eta));
        });
    });

    match result {
        Ok((path, hashes)) => {
            let elapsed = meter.start.elapsed().as_secs();
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let summary = if elapsed > 0 && bytes > 0 {
                format!(
                    "Downloaded {} in {} ({} average)",
                    usbooty_core::device::format_size(bytes),
                    format_duration(elapsed),
                    format_rate(bytes as f64 / elapsed as f64),
                )
            } else {
                "Windows ISO downloaded".to_string()
            };
            let path = path.to_string_lossy().into_owned();
            apply(&qt, ProgressMsg::info(format!("{summary} → {path}")));
            apply(
                &qt,
                ProgressMsg::info(format!("SHA-256: {}", hashes.sha256)),
            );
            // Analyze on this worker thread; both `analyze` and the WIM
            // metadata read are disk-bound and would freeze the UI if done
            // inside the queued closure.
            let report = crate::iso::analyze(std::path::Path::new(&path));
            let win = (report.os_kind == usbooty_core::OsKind::Windows)
                .then(|| crate::iso::windows_meta(std::path::Path::new(&path)));
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_progress(1.0);
                ctrl.as_mut().set_phase(QString::from("Finished"));
                ctrl.as_mut().set_speed(QString::default());
                ctrl.as_mut().set_eta(QString::default());
                ctrl.as_mut().set_status(QString::from(&summary));
                // Every digest was computed during the download; use them
                // directly instead of re-reading the whole ISO.
                ctrl.as_mut().apply_iso(&path, report, win, Some(&hashes));
                // Clear the job handle like finish() does, so a stray Cancel
                // click while idle doesn't flip a dead abort flag.
                ctrl.as_mut().rust_mut().job = None;
                ctrl.as_mut().job_finished(true, QString::from(&summary));
            });
        }
        Err(e) => finish(&qt, false, format!("{e:#}")),
    }
}

/// Set the UI phase label from a worker thread.
fn set_phase(qt: &CxxQtThread<AppController>, name: &str) {
    let name = name.to_string();
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_phase(QString::from(&name));
    });
}

/// Format one activity-log line as an inline rich-text fragment. Warnings
/// (amber) and errors (red) stand out against the default info colour, and
/// `$ command` lines (the helper echoing every external tool it runs) render
/// teal so the eye can separate "what usbooty did" from "what it said". Each
/// line is HTML-escaped before styling so file paths or messages containing
/// `<`, `>`, `&` render verbatim. The caller (`push_log_line`) wraps the
/// fragment in a timestamped `<div>`, so no block element here.
pub(crate) fn log_html(level: LogLevel, text: &str) -> String {
    let escaped = html_escape(text);
    match level {
        LogLevel::Info if text.starts_with("$ ") => {
            format!("<span style=\"color:#1b9aaa\">{escaped}</span>")
        }
        LogLevel::Info => escaped,
        LogLevel::Warn => format!("<span style=\"color:#d18616\">\u{26A0} {escaped}</span>"),
        LogLevel::Error => format!("<span style=\"color:#e5534b\">\u{2717} {escaped}</span>"),
    }
}

/// The closing green/red summary line appended when a job ends, so the log
/// itself records the outcome (the status bar and result dialog are
/// transient; "Save log" keeps this).
pub(crate) fn outcome_html(success: bool, text: &str) -> String {
    let escaped = html_escape(text);
    if success {
        format!("<span style=\"color:#2da44e;font-weight:bold\">\u{2713} {escaped}</span>")
    } else {
        format!("<span style=\"color:#e5534b;font-weight:bold\">\u{2717} {escaped}</span>")
    }
}

/// Append one log line. The plain text is kept Rust-side for "Save log" and
/// the HTML is appended incrementally by the QML view (see
/// `AppController::push_log_line`), so each append is O(line length) rather
/// than rebuilding the whole buffer.
fn append_log(ctrl: Pin<&mut AppController>, level: LogLevel, text: &str) {
    ctrl.push_log_line(text, &log_html(level, text));
}

/// Visually mark a new phase in the log with a bold, blue header line.
/// Suppressed for the implicit "Starting" phase that every job begins in.
fn append_phase_header(ctrl: Pin<&mut AppController>, name: &str) {
    if name.is_empty() || name.eq_ignore_ascii_case("Starting") {
        return;
    }
    let escaped = html_escape(name);
    let html = format!(
        "<span style=\"color:#2188ff;font-weight:bold\">\u{25B6} {escaped}</span>"
    );
    ctrl.push_log_line(&format!("== {name} =="), &html);
}

/// Minimal HTML escaping for log text (the four characters Qt's rich-text
/// parser treats specially).
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Compute all of the source ISO's digests on a worker thread and publish them
/// to the matching `iso_*` properties. Slow for a multi-gigabyte ISO (disk
/// bound), hence off-thread. The `hash_progress` property is updated as
/// fractions of completion so the UI can show a percent instead of a
/// frozen "Computing…".
///
/// `generation` is the value of `hash_generation` when this worker was
/// spawned; every queued closure re-checks it so a worker outlived by an
/// ISO change discards its results instead of publishing them under the
/// wrong image.
pub fn compute_iso_hashes(qt: CxxQtThread<AppController>, path: String, generation: u64) {
    use crate::iso::HashKind;

    let qt_progress = qt.clone();
    let qt_hash = qt.clone();
    let hashes = crate::iso::compute_hashes(
        std::path::Path::new(&path),
        move |done, total| {
            let fraction = if total > 0 {
                (done as f64 / total as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let _ = qt_progress.queue(move |mut ctrl: Pin<&mut AppController>| {
                if ctrl.rust().hash_generation != generation {
                    return;
                }
                ctrl.as_mut().set_hash_progress(fraction);
            });
        },
        // Each digest lands here as its worker finishes; publish it to the
        // matching property so the UI swaps that hash's spinner for the value
        // immediately, without waiting for the slower hashes.
        move |kind, digest| {
            let _ = qt_hash.queue(move |mut ctrl: Pin<&mut AppController>| {
                if ctrl.rust().hash_generation != generation {
                    return;
                }
                let v = QString::from(&digest);
                match kind {
                    HashKind::Md5 => ctrl.as_mut().set_iso_md5(v),
                    HashKind::Sha1 => ctrl.as_mut().set_iso_sha1(v),
                    HashKind::Sha256 => ctrl.as_mut().set_iso_sha256(v),
                    HashKind::Sha512 => ctrl.as_mut().set_iso_sha512(v),
                    HashKind::Blake3 => ctrl.as_mut().set_iso_blake3(v),
                }
            });
        },
    );
    // SHA-1 in hand: optionally cross-check against the rg-adguard database.
    // The lookup is bounded by a short HTTP timeout and any failure path is
    // silent, so it never holds up the hash display.
    let sha1 = hashes.sha1.clone();
    let badge = crate::rgadguard::lookup(&sha1)
        .map(|v| v.badge())
        .unwrap_or_default();

    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        if ctrl.rust().hash_generation != generation {
            return;
        }
        ctrl.as_mut().set_iso_adguard_badge(QString::from(&badge));
        ctrl.as_mut().set_hash_progress(1.0);
        ctrl.as_mut().set_hashing(false);
    });
}

/// Decompress a compressed source image on a worker thread, then hand the
/// resulting plain image off to the regular `set_iso` path. Progress is
/// reported on the same Qt `phase`/`progress` properties the helper drives,
/// so the UI looks like a normal write-phase to the user.
/// Analyze a plain ISO off the Qt thread and queue the result back. Replaces
/// the previous synchronous `iso::analyze` call inside `set_iso` so picking
/// a multi-GB source never freezes the UI. Also the common landing point for
/// `decompress_then_analyze` and `strip_vhd_then_analyze` after they finish
/// producing a plain file.
pub fn analyze_then_apply(
    qt: CxxQtThread<AppController>,
    src: PathBuf,
    abort: Option<Arc<std::sync::atomic::AtomicBool>>,
) {
    let display = src.display().to_string();
    let report = crate::iso::analyze(&src);
    // Pre-compute the Windows WIM metadata here too: it re-reads the WIM XML
    // (disk bound) and must not run inside the Qt-thread closure.
    let win = (report.os_kind == usbooty_core::OsKind::Windows)
        .then(|| crate::iso::windows_meta(&src));
    // Analysis itself is bounded (seconds), so it runs to completion; a
    // cancel clicked during it discards the result instead of applying it.
    let cancelled = abort.is_some_and(|a| a.load(std::sync::atomic::Ordering::SeqCst));
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_busy(false);
        ctrl.as_mut().set_progress(0.0);
        ctrl.as_mut().set_phase(QString::default());
        ctrl.as_mut().set_speed(QString::default());
        ctrl.as_mut().set_eta(QString::default());
        ctrl.as_mut().rust_mut().job = None;
        if cancelled {
            ctrl.as_mut().set_status(QString::from("Cancelled."));
            return;
        }
        ctrl.as_mut().set_status(QString::from("Ready"));
        ctrl.as_mut().apply_iso(&display, report, win, None);
    });
}

pub fn decompress_then_analyze(
    qt: CxxQtThread<AppController>,
    src: PathBuf,
    abort: Arc<std::sync::atomic::AtomicBool>,
) {
    let display = src.display().to_string();
    apply(&qt, ProgressMsg::info(format!("Decompressing {display} …")));

    let qt_for_progress = qt.clone();
    let mut meter = RateMeter::new();
    let result = crate::decompress::decompress_to_cache(&src, &abort, move |consumed, total| {
        let rate = meter.sample(consumed);
        let fraction = if total > 0 {
            consumed as f64 / total as f64
        } else {
            0.0
        };
        let speed = format_rate(rate);
        let eta = eta_string(rate, consumed, total);
        let _ = qt_for_progress.queue(move |mut ctrl: Pin<&mut AppController>| {
            ctrl.as_mut().set_progress(fraction);
            ctrl.as_mut().set_phase(QString::from("Decompressing"));
            ctrl.as_mut().set_speed(QString::from(&speed));
            ctrl.as_mut().set_eta(QString::from(&eta));
        });
    });

    match result {
        Ok(path) => {
            apply(
                &qt,
                ProgressMsg::info(format!("Decompressed → {}", path.display())),
            );
            // Drop straight into off-thread analysis with the decompressed
            // file. Going through `set_iso` would spawn yet another worker
            // for the same job; calling `analyze_then_apply` directly skips
            // the extra hop.
            analyze_then_apply(qt, path, Some(abort));
        }
        Err(e) => {
            let message = format!("Could not decompress {display}: {e:#}");
            apply(
                &qt,
                ProgressMsg::Error {
                    text: message.clone(),
                },
            );
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_progress(0.0);
                ctrl.as_mut().set_phase(QString::default());
                ctrl.as_mut().set_speed(QString::default());
                ctrl.as_mut().set_eta(QString::default());
                ctrl.as_mut().set_iso_summary(QString::from(&message));
                ctrl.as_mut().set_status(QString::from(&message));
                ctrl.as_mut().rust_mut().job = None;
            });
        }
    }
}

/// Strip the 512-byte footer off a fixed VHD on a worker thread, then
/// re-enter `set_iso` with the resulting `.img`. Dynamic / differencing
/// VHDs surface a clear error explaining how to convert them. The progress
/// surface is shared with the decompression path; the user sees an
/// "Unwrapping VHD" phase while the cache file is written.
pub fn strip_vhd_then_analyze(
    qt: CxxQtThread<AppController>,
    src: PathBuf,
    abort: Arc<std::sync::atomic::AtomicBool>,
) {
    let display = src.display().to_string();
    apply(
        &qt,
        ProgressMsg::info(format!("Unwrapping fixed VHD {display} …")),
    );
    let _ = qt.queue(|mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_phase(QString::from("Unwrapping VHD"));
        ctrl.as_mut().set_progress(0.0);
    });

    match crate::vhd::strip_footer_to_cache(&src, &abort) {
        Ok(path) => {
            apply(
                &qt,
                ProgressMsg::info(format!("Unwrapped → {}", path.display())),
            );
            // Same shortcut as the decompression path: skip the `set_iso`
            // round-trip and analyze the unwrapped image directly.
            analyze_then_apply(qt, path, Some(abort));
        }
        Err(e) => {
            let message = format!("Could not open VHD {display}: {e:#}");
            apply(
                &qt,
                ProgressMsg::Error {
                    text: message.clone(),
                },
            );
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_progress(0.0);
                ctrl.as_mut().set_phase(QString::default());
                ctrl.as_mut().set_iso_summary(QString::from(&message));
                ctrl.as_mut().set_status(QString::from(&message));
                ctrl.as_mut().rust_mut().job = None;
            });
        }
    }
}
