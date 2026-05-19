//! The line-delimited JSON protocol the privileged helper streams on stdout.
//!
//! The helper prints exactly one [`ProgressMsg`] per line. The GUI's `runner`
//! module parses each line and forwards it onto the Qt thread.

use serde::{Deserialize, Serialize};

/// Severity of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// A single message from the helper to the GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressMsg {
    /// A human-readable log line for the GUI's log view.
    Log { level: LogLevel, text: String },
    /// The helper has entered a new named phase (e.g. `Writing`, `Verifying`).
    Phase { name: String },
    /// Fine-grained progress within the current phase.
    Progress {
        phase: String,
        done: u64,
        total: u64,
    },
    /// The job finished successfully.
    Done,
    /// The job failed; `text` explains why.
    Error { text: String },
}

impl ProgressMsg {
    /// Convenience constructor for an info-level log line.
    pub fn info(text: impl Into<String>) -> Self {
        ProgressMsg::Log {
            level: LogLevel::Info,
            text: text.into(),
        }
    }

    /// Convenience constructor for a warning log line.
    pub fn warn(text: impl Into<String>) -> Self {
        ProgressMsg::Log {
            level: LogLevel::Warn,
            text: text.into(),
        }
    }

    /// Serialize to a single JSON line (no trailing newline).
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("ProgressMsg is always serializable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let msg = ProgressMsg::Progress {
            phase: "Writing".into(),
            done: 1024,
            total: 4096,
        };
        let line = msg.to_line();
        let back: ProgressMsg = serde_json::from_str(&line).unwrap();
        assert_eq!(msg, back);
    }
}
