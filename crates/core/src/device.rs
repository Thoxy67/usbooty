//! Description of a candidate target block device, as enumerated by the GUI.

use serde::{Deserialize, Serialize};

/// A block device the user could write to (typically a removable USB drive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Kernel device node, e.g. `/dev/sdb`.
    pub path: String,
    /// Human-readable vendor/model string, e.g. `SanDisk Ultra`.
    pub model: String,
    /// Total capacity in bytes.
    pub size: u64,
    /// Whether the kernel reports the device as removable.
    pub removable: bool,
}

impl DeviceInfo {
    /// A label for the device picker, e.g.
    /// `SanDisk Ultra — 30.8 GB · Removable · /dev/sdb`.
    ///
    /// The em-dash separates a primary part (the model) from a detail part
    /// (capacity, bus kind, node); the QML delegate splits on it to render the
    /// two on separate rows.
    pub fn display(&self) -> String {
        format!("{} — {}", self.model_name(), self.detail())
    }

    /// The model name, falling back to a placeholder when the kernel reports none.
    pub fn model_name(&self) -> &str {
        let model = self.model.trim();
        if model.is_empty() {
            "Unknown device"
        } else {
            model
        }
    }

    /// The secondary detail line: capacity, bus kind, and device node.
    pub fn detail(&self) -> String {
        let kind = if self.removable {
            "Removable"
        } else {
            "Internal disk"
        };
        format!("{} · {kind} · {}", format_size(self.size), self.path)
    }
}

/// Format a byte count as a short decimal (SI) size string.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(30_752_000_000), "30.8 GB");
    }
}
