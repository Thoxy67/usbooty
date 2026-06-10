//! Stdout side of the helper→GUI protocol.
//!
//! Every [`ProgressMsg`] is written as one JSON line to stdout and flushed
//! immediately so the GUI sees progress in real time.
//!
//! Progress updates are throttled at 0.1 % granularity: callers in the write
//! loops fire `progress()` on every 4 MiB chunk (~1024 times for a 4 GiB
//! write), but the GUI only needs to redraw when the displayed percentage
//! actually changes. Log / phase / error messages always pass through.

use std::io::Write;
use std::sync::Mutex;
use usbooty_core::{LogLevel, ProgressMsg};

/// Serializes access to stdout so messages never interleave.
static STDOUT_LOCK: Mutex<()> = Mutex::new(());

/// The last `(phase, pct_x10)` we actually emitted, so duplicate progress
/// messages (same phase, same percent rounded to 0.1 %) are dropped at the
/// emit boundary instead of clogging the stdout pipe.
static LAST_PROGRESS: Mutex<Option<(String, u32)>> = Mutex::new(None);

/// Write a single [`ProgressMsg`] to stdout as a JSON line.
pub fn emit(msg: &ProgressMsg) {
    let _guard = STDOUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", msg.to_line());
    let _ = out.flush();
}

/// Emit an info-level log line.
pub fn log(text: impl Into<String>) {
    emit(&ProgressMsg::info(text));
}

/// Emit a warning log line.
pub fn warn(text: impl Into<String>) {
    emit(&ProgressMsg::warn(text));
}

/// Emit a phase transition.
pub fn phase(name: impl Into<String>) {
    emit(&ProgressMsg::Phase { name: name.into() });
}

/// Log an external command invocation as a `$ program args...` line, right
/// before it is spawned. Makes every tool the helper shells out to (mkfs,
/// ventoy, wimlib-imagex, mount, ...) visible and reproducible from the
/// activity log.
pub fn cmd(c: &std::process::Command) {
    let mut line = String::from("$ ");
    line.push_str(&c.get_program().to_string_lossy());
    for a in c.get_args() {
        line.push(' ');
        let a = a.to_string_lossy();
        // Quote arguments with whitespace so the logged line stays
        // copy-pasteable into a shell.
        if a.contains(char::is_whitespace) {
            line.push('\'');
            line.push_str(&a);
            line.push('\'');
        } else {
            line.push_str(&a);
        }
    }
    log(line);
}

/// Emit a terminal error message.
pub fn error(text: impl Into<String>) {
    emit(&ProgressMsg::Error { text: text.into() });
}

/// Emit a phase-scoped progress update, with 0.1 % deduplication.
///
/// Callers in the write loops fire this on every 4 MiB chunk. For a fast
/// USB 3 device that's ~1024 calls for a 4 GiB write; deduping at 0.1 %
/// granularity collapses that to ~1000 messages, one per 0.1 % step.
/// A slow stick or a small image with one chunk per percent still emits
/// every update.
pub fn progress(phase: &str, done: u64, total: u64) {
    let pct_x10 = done
        .saturating_mul(1000)
        .checked_div(total)
        .unwrap_or(0)
        .min(1000) as u32;
    {
        let mut last = LAST_PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        match &*last {
            Some((p, pct)) if p == phase && *pct == pct_x10 => return,
            _ => *last = Some((phase.to_string(), pct_x10)),
        }
    }
    emit(&ProgressMsg::Progress {
        phase: phase.to_string(),
        done,
        total,
    });
}

/// Emit a log line at an explicit level.
#[allow(dead_code)] // used by later milestones
pub fn log_at(level: LogLevel, text: impl Into<String>) {
    emit(&ProgressMsg::Log {
        level,
        text: text.into(),
    });
}
