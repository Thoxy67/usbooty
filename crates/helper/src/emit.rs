//! Stdout side of the helper→GUI protocol.
//!
//! Every [`ProgressMsg`] is written as one JSON line to stdout and flushed
//! immediately so the GUI sees progress in real time.

use std::io::Write;
use std::sync::Mutex;
use usbooty_core::{LogLevel, ProgressMsg};

/// Serializes access to stdout so messages never interleave.
static STDOUT_LOCK: Mutex<()> = Mutex::new(());

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

/// Emit a terminal error message.
pub fn error(text: impl Into<String>) {
    emit(&ProgressMsg::Error { text: text.into() });
}

/// Emit a phase-scoped progress update.
pub fn progress(phase: &str, done: u64, total: u64) {
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
