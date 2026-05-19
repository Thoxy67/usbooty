//! Spawns the privileged helper via `pkexec` and pumps its progress stream
//! back onto the Qt thread.
//!
//! This runs entirely on a worker thread. It never touches QObject state
//! directly — every UI mutation is marshalled through [`CxxQtThread::queue`],
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(local) = exe.parent().map(|d| d.join("usbooty-helper")) {
            if local.is_file() {
                return local;
            }
        }
    }
    PathBuf::from(INSTALLED_HELPER)
}

/// A smoothed transfer-rate estimator shared by the writer and the downloader.
struct RateMeter {
    /// When the transfer began — the basis for elapsed time and average rate.
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

    /// Feed the latest cumulative byte count and return the smoothed rate.
    /// A drop in `done` — a new phase that restarts at zero — resets the
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
pub fn run_job(
    mut job: Job,
    qt: CxxQtThread<AppController>,
    stdin_slot: Arc<Mutex<Option<ChildStdin>>>,
) {
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

    // Send the job as one JSON line, then keep stdin open so `cancel` can be
    // written to it later by AppController::cancel.
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let job_line = serde_json::to_string(&job).unwrap_or_default();
    if writeln!(stdin, "{job_line}")
        .and_then(|_| stdin.flush())
        .is_err()
    {
        finish(&qt, false, "Could not send the job to the helper".into());
        return;
    }
    *stdin_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(stdin);

    // Stream stdout, forwarding each progress message to the UI.
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut last_error: Option<String> = None;
    let mut saw_done = false;
    let mut meter = RateMeter::new();
    let mut moved = 0u64;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<ProgressMsg>(&line) else {
            continue; // ignore non-protocol noise
        };
        match &msg {
            ProgressMsg::Done => saw_done = true,
            ProgressMsg::Error { text } => last_error = Some(text.clone()),
            ProgressMsg::Progress { done, total, .. } => {
                moved = moved.max(*done);
                let rate = meter.sample(*done);
                push_stats(&qt, rate, *done, *total);
            }
            _ => {}
        }
        apply(&qt, msg);
    }

    let (success, message) = outcome(&mut child, saw_done, last_error);
    let message = finish_summary(success, message, &meter, moved);
    finish(&qt, success, message);
}

/// Append elapsed time and average rate to a successful job's message.
fn finish_summary(success: bool, message: String, meter: &RateMeter, moved: u64) -> String {
    let elapsed = meter.start.elapsed().as_secs();
    if success && moved > 0 && elapsed > 0 {
        format!(
            "{message} — {} elapsed, {} average",
            format_duration(elapsed),
            format_rate(moved as f64 / elapsed as f64),
        )
    } else {
        message
    }
}

/// Wait for the helper and decide the final success flag and message.
fn outcome(child: &mut Child, saw_done: bool, last_error: Option<String>) -> (bool, String) {
    let status = child.wait();
    let success = saw_done && matches!(&status, Ok(s) if s.success());

    if success {
        return (true, "Done — the drive is ready".into());
    }
    if let Some(err) = last_error {
        return (false, err);
    }

    let mut stderr = String::new();
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut stderr);
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
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_busy(false);
        ctrl.as_mut()
            .set_phase(QString::from(if success { "Finished" } else { "Failed" }));
        if success {
            ctrl.as_mut().set_progress(1.0);
        }
        ctrl.as_mut().set_speed(QString::default());
        ctrl.as_mut().set_eta(QString::default());
        ctrl.as_mut().set_status(QString::from(&message));
        ctrl.as_mut().rust_mut().job = None;
        ctrl.as_mut().job_finished(success, QString::from(&message));
    });
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

/// Download a Windows ISO from `url` and select it as the source.
pub fn download_windows_url(qt: CxxQtThread<AppController>, url: String) {
    set_phase(&qt, "Downloading Windows ISO");
    apply(&qt, ProgressMsg::info("Downloading the Windows ISO from Microsoft…"));

    let dest_dir = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(std::path::Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir);

    let abort = std::sync::atomic::AtomicBool::new(false);
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
        Ok((path, sha256)) => {
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
            apply(&qt, ProgressMsg::info(format!("SHA-256: {sha256}")));
            let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
                ctrl.as_mut().set_busy(false);
                ctrl.as_mut().set_progress(1.0);
                ctrl.as_mut().set_phase(QString::from("Finished"));
                ctrl.as_mut().set_speed(QString::default());
                ctrl.as_mut().set_eta(QString::default());
                ctrl.as_mut().set_status(QString::from(&summary));
                // The SHA-256 was computed during the download — use it
                // directly instead of re-reading the whole ISO.
                ctrl.as_mut().set_downloaded_iso(&path, &sha256);
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

/// Append one line to the controller's log text.
fn append_log(mut ctrl: Pin<&mut AppController>, level: LogLevel, text: &str) {
    let prefix = match level {
        LogLevel::Info => "",
        LogLevel::Warn => "⚠ ",
        LogLevel::Error => "✗ ",
    };
    let updated = format!("{}{prefix}{text}\n", ctrl.log_text());
    ctrl.as_mut().set_log_text(QString::from(&updated));
}

/// Compute the source ISO's SHA-256 on a worker thread and publish it to the
/// `iso_sha256` property. Slow for a multi-gigabyte ISO, hence off-thread.
pub fn compute_iso_sha256(qt: CxxQtThread<AppController>, path: String) {
    let hash = crate::iso::sha256(std::path::Path::new(&path));
    let _ = qt.queue(move |mut ctrl: Pin<&mut AppController>| {
        ctrl.as_mut().set_iso_sha256(QString::from(&hash));
    });
}
